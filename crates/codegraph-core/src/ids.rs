use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    dirty_evidence::ProofLadderLevel, validation::ValidationClassification, EntityKind,
    EvidenceRole, RelationKind, SourceSpan,
};

pub const MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION: &str =
    "mvp4.2-typescript-local-returns-to-v1";
pub const MVP4_2_LOCAL_RETURNS_TO_CLAIMABILITY: &str =
    "claimable_source_spanned_local_return_containment";
pub const MVP4_2B_LOCAL_READS_EXTRACTION_VERSION: &str = "mvp4.2b-typescript-local-reads-v1";
pub const MVP4_2B_LOCAL_READS_CLAIMABILITY: &str =
    "claimable_source_spanned_resolver_proven_local_read";
pub const MVP4_2B_LOCAL_WRITES_EXTRACTION_VERSION: &str = "mvp4.2b-typescript-local-writes-v1";
pub const MVP4_2B_LOCAL_WRITES_CLAIMABILITY: &str =
    "claimable_source_spanned_resolver_proven_local_write";
pub const MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION: &str = "mvp4.2b-typescript-local-flows-to-v1";
pub const MVP4_2B_LOCAL_FLOWS_TO_CLAIMABILITY: &str =
    "claimable_source_spanned_derived_local_value_flow";
pub const MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION: u32 = 1;
pub const MVP4_2_MICRO_EDGE_PAYLOAD_VERSION: u32 = 1;
pub const MVP4_3_LOCAL_MICRO_FLOW_PACKET_SCHEMA_VERSION: u32 = 1;
pub const MVP4_3_LOCAL_MICRO_FLOW_PACKET_PAYLOAD_VERSION: u32 = 1;
pub const MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION: &str =
    "mvp4.3-typescript-local-micro-flow-packets-v1";
pub const MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND: &str = "function_local_micro_flow_packet";
pub const MVP4_3_LOCAL_MICRO_FLOW_PACKET_ENCODING: &str = "dict_v1";

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
    /// A source-spanned read/use occurrence of a value binding (identifier in
    /// read position). Existence-only until a local binding resolver proves the
    /// binding (MVP4.2b); the missing read endpoint for `LOCAL_READS`/flow.
    ValueUse,
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
        Self::ValueUse,
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
            Self::ValueUse => "value_use",
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

    pub fn from_storage_str(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "local_reads" => Some(Self::LocalReads),
            "local_writes" => Some(Self::LocalWrites),
            "local_flows_to" => Some(Self::LocalFlowsTo),
            "local_calls" => Some(Self::LocalCalls),
            "local_returns_to" => Some(Self::LocalReturnsTo),
            "local_mutates" => Some(Self::LocalMutates),
            "local_checks" => Some(Self::LocalChecks),
            "local_sanitizes" => Some(Self::LocalSanitizes),
            "local_guards" => Some(Self::LocalGuards),
            "local_asserts" => Some(Self::LocalAsserts),
            "local_branches_to" => Some(Self::LocalBranchesTo),
            _ => None,
        }
    }
}

impl fmt::Display for MicroEdgeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for MicroEdgeKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_storage_str(value).ok_or(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicroEdgeSupportStatus {
    ExactCapable,
    DerivedWithProvenanceCapable,
    RequiresLocalBindingResolver,
    RequiresTypeResolver,
    RequiresCompilerOrLsp,
    RequiresMacroExpansion,
    HeuristicOnly,
    Unsupported,
    Unknown,
    NotImplemented,
}

impl MicroEdgeSupportStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactCapable => "exact_capable",
            Self::DerivedWithProvenanceCapable => "derived_with_provenance_capable",
            Self::RequiresLocalBindingResolver => "requires_local_binding_resolver",
            Self::RequiresTypeResolver => "requires_type_resolver",
            Self::RequiresCompilerOrLsp => "requires_compiler_or_lsp",
            Self::RequiresMacroExpansion => "requires_macro_expansion",
            Self::HeuristicOnly => "heuristic_only",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
            Self::NotImplemented => "not_implemented",
        }
    }

    pub const fn default_exact_support(self) -> bool {
        matches!(self, Self::ExactCapable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MicroEdgeLanguageCapability {
    pub language: &'static str,
    pub frontend: Option<&'static str>,
    pub micro_edge_kind: MicroEdgeKind,
    pub activation_status: MicroEdgeSupportStatus,
    pub endpoint_node_requirements: &'static [MicroNodeKind],
    pub resolver_requirements: &'static [&'static str],
    pub exactness_capability: MicroExactness,
    pub provenance_builder: &'static str,
    pub parser_recovery_behavior: &'static str,
    pub source_role_behavior: &'static str,
    pub claimability_label: Option<&'static str>,
    pub fixture_ids: &'static [&'static str],
    pub extraction_version: Option<&'static str>,
    pub unsupported_reason: Option<&'static str>,
}

impl MicroEdgeLanguageCapability {
    pub const fn not_implemented(kind: MicroEdgeKind) -> Self {
        Self {
            language: "any",
            frontend: None,
            micro_edge_kind: kind,
            activation_status: MicroEdgeSupportStatus::NotImplemented,
            endpoint_node_requirements: &[],
            resolver_requirements: &[],
            exactness_capability: MicroExactness::Unsupported,
            provenance_builder: "none",
            parser_recovery_behavior: "unsupported",
            source_role_behavior: "not_applicable",
            claimability_label: None,
            fixture_ids: &[],
            extraction_version: None,
            unsupported_reason: Some(
                "no micro-edge adapter is implemented for this language/relation",
            ),
        }
    }

    pub fn matches_frontend(self, frontend: &str) -> bool {
        match self.frontend {
            Some(expected) => frontend.trim() == expected,
            None => true,
        }
    }

    pub fn supports_claimable_exact(
        self,
        frontend: &str,
        source_role: MicroSourceRole,
        exactness: MicroExactness,
        claimability: &str,
        extraction_version: &str,
    ) -> bool {
        self.activation_status == MicroEdgeSupportStatus::ExactCapable
            && self.matches_frontend(frontend)
            && source_role == MicroSourceRole::Production
            && exactness == MicroExactness::Exact
            && self
                .claimability_label
                .is_some_and(|expected| claimability.trim() == expected)
            && self
                .extraction_version
                .is_some_and(|expected| extraction_version.trim() == expected)
    }

    /// Like `supports_claimable_exact`, but honors the capability's own
    /// activation contract: exact-capable relations require `Exact` rows,
    /// while derived-with-provenance relations (LOCAL_FLOWS_TO) require
    /// `DerivedWithProvenance` rows. Claimability label and extraction
    /// version must match either way; every other status is never claimable.
    pub fn supports_claimable_proof_row(
        self,
        frontend: &str,
        source_role: MicroSourceRole,
        exactness: MicroExactness,
        claimability: &str,
        extraction_version: &str,
    ) -> bool {
        let exactness_matches_activation = match self.activation_status {
            MicroEdgeSupportStatus::ExactCapable => exactness == MicroExactness::Exact,
            MicroEdgeSupportStatus::DerivedWithProvenanceCapable => {
                exactness == MicroExactness::DerivedWithProvenance
            }
            _ => return false,
        };
        exactness_matches_activation
            && self.matches_frontend(frontend)
            && source_role == MicroSourceRole::Production
            && self
                .claimability_label
                .is_some_and(|expected| claimability.trim() == expected)
            && self
                .extraction_version
                .is_some_and(|expected| extraction_version.trim() == expected)
    }
}

const LOCAL_RETURNS_TO_ENDPOINT_REQUIREMENTS: &[MicroNodeKind] =
    &[MicroNodeKind::ReturnSite, MicroNodeKind::FunctionFrame];
const LOCAL_RETURNS_TO_RESOLVER_REQUIREMENTS: &[&str] =
    &["nearest_enclosing_function_scope_ownership"];
const LOCAL_RETURNS_TO_FIXTURE_IDS: &[&str] = &[
    "ts_local_returns_to_simple_expression_return",
    "ts_local_returns_to_bare_return",
    "ts_local_returns_to_multiple_returns",
    "ts_local_returns_to_nested_function_nearest_owner",
];

pub const MVP4_TYPESCRIPT_LOCAL_RETURNS_TO_CAPABILITY: MicroEdgeLanguageCapability =
    MicroEdgeLanguageCapability {
        language: "typescript",
        frontend: Some("tree-sitter-typescript"),
        micro_edge_kind: MicroEdgeKind::LocalReturnsTo,
        activation_status: MicroEdgeSupportStatus::ExactCapable,
        endpoint_node_requirements: LOCAL_RETURNS_TO_ENDPOINT_REQUIREMENTS,
        resolver_requirements: LOCAL_RETURNS_TO_RESOLVER_REQUIREMENTS,
        exactness_capability: MicroExactness::Exact,
        provenance_builder: "direct_ast_nearest_enclosing_function_ownership",
        parser_recovery_behavior: "omit_exact_edge_on_recovery_ambiguity",
        source_role_behavior: "production_only",
        claimability_label: Some(MVP4_2_LOCAL_RETURNS_TO_CLAIMABILITY),
        fixture_ids: LOCAL_RETURNS_TO_FIXTURE_IDS,
        extraction_version: Some(MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION),
        unsupported_reason: None,
    };

const LOCAL_BINDING_RESOLVER_REQUIREMENTS: &[&str] =
    &["local_binding_resolver_lexical_scope_shadowing"];
const LOCAL_READS_ENDPOINT_REQUIREMENTS: &[MicroNodeKind] = &[
    MicroNodeKind::ValueUse,
    MicroNodeKind::Parameter,
    MicroNodeKind::LocalBinding,
];
const LOCAL_WRITES_ENDPOINT_REQUIREMENTS: &[MicroNodeKind] = &[
    MicroNodeKind::AssignmentSite,
    MicroNodeKind::Parameter,
    MicroNodeKind::LocalBinding,
];
const LOCAL_READS_FIXTURE_IDS: &[&str] = &[];
const LOCAL_WRITES_FIXTURE_IDS: &[&str] = &[];

pub const MVP4_TYPESCRIPT_LOCAL_READS_CAPABILITY: MicroEdgeLanguageCapability =
    MicroEdgeLanguageCapability {
        language: "typescript",
        frontend: Some("tree-sitter-typescript"),
        micro_edge_kind: MicroEdgeKind::LocalReads,
        activation_status: MicroEdgeSupportStatus::ExactCapable,
        endpoint_node_requirements: LOCAL_READS_ENDPOINT_REQUIREMENTS,
        resolver_requirements: LOCAL_BINDING_RESOLVER_REQUIREMENTS,
        exactness_capability: MicroExactness::Exact,
        provenance_builder: "direct_ast_resolver_proven_local_read",
        parser_recovery_behavior: "omit_exact_edge_on_recovery_ambiguity",
        source_role_behavior: "production_only",
        claimability_label: Some(MVP4_2B_LOCAL_READS_CLAIMABILITY),
        fixture_ids: LOCAL_READS_FIXTURE_IDS,
        extraction_version: Some(MVP4_2B_LOCAL_READS_EXTRACTION_VERSION),
        unsupported_reason: None,
    };

pub const MVP4_TYPESCRIPT_LOCAL_WRITES_CAPABILITY: MicroEdgeLanguageCapability =
    MicroEdgeLanguageCapability {
        language: "typescript",
        frontend: Some("tree-sitter-typescript"),
        micro_edge_kind: MicroEdgeKind::LocalWrites,
        activation_status: MicroEdgeSupportStatus::ExactCapable,
        endpoint_node_requirements: LOCAL_WRITES_ENDPOINT_REQUIREMENTS,
        resolver_requirements: LOCAL_BINDING_RESOLVER_REQUIREMENTS,
        exactness_capability: MicroExactness::Exact,
        provenance_builder: "direct_ast_resolver_proven_local_write",
        parser_recovery_behavior: "omit_exact_edge_on_recovery_ambiguity",
        source_role_behavior: "production_only",
        claimability_label: Some(MVP4_2B_LOCAL_WRITES_CLAIMABILITY),
        fixture_ids: LOCAL_WRITES_FIXTURE_IDS,
        extraction_version: Some(MVP4_2B_LOCAL_WRITES_EXTRACTION_VERSION),
        unsupported_reason: None,
    };

// LOCAL_FLOWS_TO is a DERIVED edge (not direct AST): for a write `target = expr`,
// each binding read in `expr` flows into the write target. Endpoints are the two
// resolver-proven bindings (source -> target), both already-persisted inventory
// nodes; the derivation cites the LOCAL_READS + LOCAL_WRITES facts as provenance.
const LOCAL_FLOWS_TO_ENDPOINT_REQUIREMENTS: &[MicroNodeKind] =
    &[MicroNodeKind::Parameter, MicroNodeKind::LocalBinding];
const LOCAL_FLOWS_TO_RESOLVER_REQUIREMENTS: &[&str] = &[
    "local_binding_resolver_lexical_scope_shadowing",
    "assignment_rhs_read_containment",
];
const LOCAL_FLOWS_TO_FIXTURE_IDS: &[&str] = &[];

pub const MVP4_TYPESCRIPT_LOCAL_FLOWS_TO_CAPABILITY: MicroEdgeLanguageCapability =
    MicroEdgeLanguageCapability {
        language: "typescript",
        frontend: Some("tree-sitter-typescript"),
        micro_edge_kind: MicroEdgeKind::LocalFlowsTo,
        activation_status: MicroEdgeSupportStatus::DerivedWithProvenanceCapable,
        endpoint_node_requirements: LOCAL_FLOWS_TO_ENDPOINT_REQUIREMENTS,
        resolver_requirements: LOCAL_FLOWS_TO_RESOLVER_REQUIREMENTS,
        exactness_capability: MicroExactness::DerivedWithProvenance,
        provenance_builder: "derived_local_assignment_chain_value_flow",
        parser_recovery_behavior: "omit_derived_edge_on_recovery_ambiguity",
        source_role_behavior: "production_only",
        claimability_label: Some(MVP4_2B_LOCAL_FLOWS_TO_CLAIMABILITY),
        fixture_ids: LOCAL_FLOWS_TO_FIXTURE_IDS,
        extraction_version: Some(MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION),
        unsupported_reason: None,
    };

pub const MVP4_ACTIVE_MICRO_EDGE_LANGUAGE_CAPABILITIES: &[MicroEdgeLanguageCapability] = &[
    MVP4_TYPESCRIPT_LOCAL_RETURNS_TO_CAPABILITY,
    MVP4_TYPESCRIPT_LOCAL_READS_CAPABILITY,
    MVP4_TYPESCRIPT_LOCAL_WRITES_CAPABILITY,
    MVP4_TYPESCRIPT_LOCAL_FLOWS_TO_CAPABILITY,
];

pub const MVP4_3_TYPESCRIPT_FIRST_SLICE_RELATIONS: &[MicroEdgeKind] = &[
    MicroEdgeKind::LocalReads,
    MicroEdgeKind::LocalWrites,
    MicroEdgeKind::LocalFlowsTo,
    MicroEdgeKind::LocalReturnsTo,
];

pub fn mvp4_micro_edge_language_capability(
    language: &str,
    kind: MicroEdgeKind,
) -> MicroEdgeLanguageCapability {
    let normalized = language.trim().to_ascii_lowercase();
    if normalized == "typescript" {
        match kind {
            MicroEdgeKind::LocalReturnsTo => return MVP4_TYPESCRIPT_LOCAL_RETURNS_TO_CAPABILITY,
            MicroEdgeKind::LocalReads => return MVP4_TYPESCRIPT_LOCAL_READS_CAPABILITY,
            MicroEdgeKind::LocalWrites => return MVP4_TYPESCRIPT_LOCAL_WRITES_CAPABILITY,
            MicroEdgeKind::LocalFlowsTo => return MVP4_TYPESCRIPT_LOCAL_FLOWS_TO_CAPABILITY,
            _ => {}
        }
    }
    MicroEdgeLanguageCapability::not_implemented(kind)
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

    pub fn from_storage_str(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "exact" => Some(Self::Exact),
            "derived_with_provenance" => Some(Self::DerivedWithProvenance),
            "heuristic" => Some(Self::Heuristic),
            "unsupported" => Some(Self::Unsupported),
            "unknown" => Some(Self::Unknown),
            _ => None,
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

    pub fn from_storage_str(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "production" => Some(Self::Production),
            "test" => Some(Self::Test),
            "mock" => Some(Self::Mock),
            "stub" => Some(Self::Stub),
            "generated" => Some(Self::Generated),
            "source_text" => Some(Self::SourceText),
            "unknown" => Some(Self::Unknown),
            "mixed" => Some(Self::Mixed),
            _ => None,
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
    /// Stable semantic step ids in packet order. These ids identify the compact
    /// path program; they are not the verbose explain/audit `ordered_steps`
    /// rendering.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroEdgeCandidateCapState {
    pub omitted_count: u64,
    pub reason: String,
    pub completeness_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroEdgeCandidate {
    pub micro_edge_id: String,
    pub micro_edge_kind: MicroEdgeKind,
    pub head_micro_node_id: String,
    pub tail_micro_node_id: String,
    pub repo_relative_path: String,
    pub function_identity: String,
    pub relation_source_span: SourceSpan,
    pub head_source_span: SourceSpan,
    pub tail_source_span: SourceSpan,
    pub provenance: MicroFactProvenance,
    pub exactness: MicroExactness,
    pub claimability: String,
    pub language: String,
    pub frontend: String,
    pub source_role: MicroSourceRole,
    pub row_schema_version: u32,
    pub payload_version: u32,
    pub extraction_version: String,
    pub cap_omission_state: Option<MicroEdgeCandidateCapState>,
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
pub enum LocalMicroFlowPacketStatus {
    MicroFlowFound,
    PartialMicroFlowFound,
    NoMicroFlowPathFound,
    MicroFlowUnavailable,
    MicroFlowTruncated,
    MicroFlowUnsupported,
    MicroFlowStale,
    MicroFlowCorrupt,
    MicroFlowIncompatible,
}

impl LocalMicroFlowPacketStatus {
    pub const ALL: &'static [Self] = &[
        Self::MicroFlowFound,
        Self::PartialMicroFlowFound,
        Self::NoMicroFlowPathFound,
        Self::MicroFlowUnavailable,
        Self::MicroFlowTruncated,
        Self::MicroFlowUnsupported,
        Self::MicroFlowStale,
        Self::MicroFlowCorrupt,
        Self::MicroFlowIncompatible,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MicroFlowFound => "micro_flow_found",
            Self::PartialMicroFlowFound => "partial_micro_flow_found",
            Self::NoMicroFlowPathFound => "no_micro_flow_path_found",
            Self::MicroFlowUnavailable => "micro_flow_unavailable",
            Self::MicroFlowTruncated => "micro_flow_truncated",
            Self::MicroFlowUnsupported => "micro_flow_unsupported",
            Self::MicroFlowStale => "micro_flow_stale",
            Self::MicroFlowCorrupt => "micro_flow_corrupt",
            Self::MicroFlowIncompatible => "micro_flow_incompatible",
        }
    }

    pub const fn rows_may_be_claimable(self) -> bool {
        matches!(self, Self::MicroFlowFound | Self::PartialMicroFlowFound)
    }

    pub const fn compact_output_allowed(self) -> bool {
        true
    }

    pub const fn validate_edit_should_warn(self) -> bool {
        matches!(
            self,
            Self::PartialMicroFlowFound
                | Self::MicroFlowUnavailable
                | Self::MicroFlowTruncated
                | Self::MicroFlowUnsupported
                | Self::MicroFlowStale
                | Self::MicroFlowCorrupt
                | Self::MicroFlowIncompatible
        )
    }

    pub const fn hard_interrupt_eligible_from_status_only(self) -> bool {
        false
    }

    pub const fn recommended_action(self) -> &'static str {
        match self {
            Self::MicroFlowFound => "use_bounded_packet_when_requested",
            Self::PartialMicroFlowFound => "treat_missing_or_unsupported_steps_as_explicit_gaps",
            Self::NoMicroFlowPathFound => "preserve_no_proof_path_state_and_fallback_evidence",
            Self::MicroFlowUnavailable => "index_or_enable_micro_flow_packet_layer_when_available",
            Self::MicroFlowTruncated => {
                "use_audit_expansion_or_reduce_scope_before_claiming_complete_flow"
            }
            Self::MicroFlowUnsupported => {
                "report_unsupported_language_or_shape_without_source_blocker"
            }
            Self::MicroFlowStale => "reindex_or_refresh_codegraph_packet_state",
            Self::MicroFlowCorrupt => "repair_or_rebuild_codegraph_packet_state",
            Self::MicroFlowIncompatible => "upgrade_or_rebuild_packet_layer_with_current_schema",
        }
    }

    pub fn from_storage_str(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "micro_flow_found" => Some(Self::MicroFlowFound),
            "partial_micro_flow_found" | "partial_local_flow" => Some(Self::PartialMicroFlowFound),
            "no_micro_flow_path_found" => Some(Self::NoMicroFlowPathFound),
            "micro_flow_unavailable" => Some(Self::MicroFlowUnavailable),
            "micro_flow_truncated" | "truncated" => Some(Self::MicroFlowTruncated),
            "micro_flow_unsupported" | "unsupported" => Some(Self::MicroFlowUnsupported),
            "micro_flow_stale" | "stale" => Some(Self::MicroFlowStale),
            "micro_flow_corrupt" | "corrupt" => Some(Self::MicroFlowCorrupt),
            "micro_flow_incompatible" | "incompatible" => Some(Self::MicroFlowIncompatible),
            _ => None,
        }
    }
}

impl fmt::Display for LocalMicroFlowPacketStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalMicroFlowProofRequirement {
    DeterministicProofBearingSteps,
    SourceSpannedProofBearingSteps,
    BackedByCurrentPersistedMicroFacts,
    DerivedStepsHaveProvenance,
    ExactStepsPreserveExactness,
    EndpointFactsCurrentAndClaimable,
    ProductionSafeSourceRole,
    BranchIdentityPreserved,
    ReturnPathIdentityPreserved,
    ShadowedBindingIdentityPreserved,
    ClaimedPathNotTruncated,
    CapOmissionsOutsideClaimedSegment,
    GapsOutsideClaimedPath,
    ClaimableLifecyclePassport,
    DictV1LosslessToAuditOrderedSteps,
    NoPacketIntegrityFinding,
    IncludesLocalFlowsTo,
    IncludesRequiredReadsAndWrites,
    NotLocalReturnsToOnly,
}

impl LocalMicroFlowProofRequirement {
    pub const ALL: &'static [Self] = &[
        Self::DeterministicProofBearingSteps,
        Self::SourceSpannedProofBearingSteps,
        Self::BackedByCurrentPersistedMicroFacts,
        Self::DerivedStepsHaveProvenance,
        Self::ExactStepsPreserveExactness,
        Self::EndpointFactsCurrentAndClaimable,
        Self::ProductionSafeSourceRole,
        Self::BranchIdentityPreserved,
        Self::ReturnPathIdentityPreserved,
        Self::ShadowedBindingIdentityPreserved,
        Self::ClaimedPathNotTruncated,
        Self::CapOmissionsOutsideClaimedSegment,
        Self::GapsOutsideClaimedPath,
        Self::ClaimableLifecyclePassport,
        Self::DictV1LosslessToAuditOrderedSteps,
        Self::NoPacketIntegrityFinding,
        Self::IncludesLocalFlowsTo,
        Self::IncludesRequiredReadsAndWrites,
        Self::NotLocalReturnsToOnly,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeterministicProofBearingSteps => "deterministic_proof_bearing_steps",
            Self::SourceSpannedProofBearingSteps => "source_spanned_proof_bearing_steps",
            Self::BackedByCurrentPersistedMicroFacts => "backed_by_current_persisted_micro_facts",
            Self::DerivedStepsHaveProvenance => "derived_steps_have_provenance",
            Self::ExactStepsPreserveExactness => "exact_steps_preserve_exactness",
            Self::EndpointFactsCurrentAndClaimable => "endpoint_facts_current_and_claimable",
            Self::ProductionSafeSourceRole => "production_safe_source_role",
            Self::BranchIdentityPreserved => "branch_identity_preserved",
            Self::ReturnPathIdentityPreserved => "return_path_identity_preserved",
            Self::ShadowedBindingIdentityPreserved => "shadowed_binding_identity_preserved",
            Self::ClaimedPathNotTruncated => "claimed_path_not_truncated",
            Self::CapOmissionsOutsideClaimedSegment => "cap_omissions_outside_claimed_segment",
            Self::GapsOutsideClaimedPath => "gaps_outside_claimed_path",
            Self::ClaimableLifecyclePassport => "claimable_lifecycle_passport",
            Self::DictV1LosslessToAuditOrderedSteps => "dict_v1_lossless_to_audit_ordered_steps",
            Self::NoPacketIntegrityFinding => "no_packet_integrity_finding",
            Self::IncludesLocalFlowsTo => "includes_local_flows_to",
            Self::IncludesRequiredReadsAndWrites => "includes_required_reads_and_writes",
            Self::NotLocalReturnsToOnly => "not_local_returns_to_only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMicroFlowProofEligibilityInput {
    pub deterministic_proof_bearing_steps: bool,
    pub source_spanned_proof_bearing_steps: bool,
    pub backed_by_current_persisted_micro_facts: bool,
    pub derived_steps_have_provenance: bool,
    pub exact_steps_preserve_exactness: bool,
    pub endpoint_facts_current_and_claimable: bool,
    pub production_safe_source_role: bool,
    pub branch_identity_preserved: bool,
    pub return_path_identity_preserved: bool,
    pub shadowed_binding_identity_preserved: bool,
    pub claimed_path_not_truncated: bool,
    pub cap_omissions_outside_claimed_segment: bool,
    pub gaps_outside_claimed_path: bool,
    pub claimable_lifecycle_passport: bool,
    pub dict_v1_lossless_to_audit_ordered_steps: bool,
    pub no_packet_integrity_finding: bool,
    pub includes_local_flows_to: bool,
    pub includes_required_reads_and_writes: bool,
    pub local_returns_to_only: bool,
    pub contains_claimable_graph_relation_summary: bool,
}

impl LocalMicroFlowProofEligibilityInput {
    pub const fn complete_local_assignment_chain() -> Self {
        Self {
            deterministic_proof_bearing_steps: true,
            source_spanned_proof_bearing_steps: true,
            backed_by_current_persisted_micro_facts: true,
            derived_steps_have_provenance: true,
            exact_steps_preserve_exactness: true,
            endpoint_facts_current_and_claimable: true,
            production_safe_source_role: true,
            branch_identity_preserved: true,
            return_path_identity_preserved: true,
            shadowed_binding_identity_preserved: true,
            claimed_path_not_truncated: true,
            cap_omissions_outside_claimed_segment: true,
            gaps_outside_claimed_path: true,
            claimable_lifecycle_passport: true,
            dict_v1_lossless_to_audit_ordered_steps: true,
            no_packet_integrity_finding: true,
            includes_local_flows_to: true,
            includes_required_reads_and_writes: true,
            local_returns_to_only: false,
            contains_claimable_graph_relation_summary: true,
        }
    }

    pub const fn local_returns_to_only() -> Self {
        Self {
            includes_local_flows_to: false,
            includes_required_reads_and_writes: false,
            local_returns_to_only: true,
            contains_claimable_graph_relation_summary: true,
            ..Self::complete_local_assignment_chain()
        }
    }

    pub fn missing_requirements(&self) -> Vec<LocalMicroFlowProofRequirement> {
        let checks = [
            (
                LocalMicroFlowProofRequirement::DeterministicProofBearingSteps,
                self.deterministic_proof_bearing_steps,
            ),
            (
                LocalMicroFlowProofRequirement::SourceSpannedProofBearingSteps,
                self.source_spanned_proof_bearing_steps,
            ),
            (
                LocalMicroFlowProofRequirement::BackedByCurrentPersistedMicroFacts,
                self.backed_by_current_persisted_micro_facts,
            ),
            (
                LocalMicroFlowProofRequirement::DerivedStepsHaveProvenance,
                self.derived_steps_have_provenance,
            ),
            (
                LocalMicroFlowProofRequirement::ExactStepsPreserveExactness,
                self.exact_steps_preserve_exactness,
            ),
            (
                LocalMicroFlowProofRequirement::EndpointFactsCurrentAndClaimable,
                self.endpoint_facts_current_and_claimable,
            ),
            (
                LocalMicroFlowProofRequirement::ProductionSafeSourceRole,
                self.production_safe_source_role,
            ),
            (
                LocalMicroFlowProofRequirement::BranchIdentityPreserved,
                self.branch_identity_preserved,
            ),
            (
                LocalMicroFlowProofRequirement::ReturnPathIdentityPreserved,
                self.return_path_identity_preserved,
            ),
            (
                LocalMicroFlowProofRequirement::ShadowedBindingIdentityPreserved,
                self.shadowed_binding_identity_preserved,
            ),
            (
                LocalMicroFlowProofRequirement::ClaimedPathNotTruncated,
                self.claimed_path_not_truncated,
            ),
            (
                LocalMicroFlowProofRequirement::CapOmissionsOutsideClaimedSegment,
                self.cap_omissions_outside_claimed_segment,
            ),
            (
                LocalMicroFlowProofRequirement::GapsOutsideClaimedPath,
                self.gaps_outside_claimed_path,
            ),
            (
                LocalMicroFlowProofRequirement::ClaimableLifecyclePassport,
                self.claimable_lifecycle_passport,
            ),
            (
                LocalMicroFlowProofRequirement::DictV1LosslessToAuditOrderedSteps,
                self.dict_v1_lossless_to_audit_ordered_steps,
            ),
            (
                LocalMicroFlowProofRequirement::NoPacketIntegrityFinding,
                self.no_packet_integrity_finding,
            ),
            (
                LocalMicroFlowProofRequirement::IncludesLocalFlowsTo,
                self.includes_local_flows_to,
            ),
            (
                LocalMicroFlowProofRequirement::IncludesRequiredReadsAndWrites,
                self.includes_required_reads_and_writes,
            ),
            (
                LocalMicroFlowProofRequirement::NotLocalReturnsToOnly,
                !self.local_returns_to_only,
            ),
        ];

        checks
            .into_iter()
            .filter_map(|(requirement, present)| (!present).then_some(requirement))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalMicroFlowProofEligibilityDecision {
    pub flow_proof_eligible: bool,
    pub packet_status: LocalMicroFlowPacketStatus,
    pub proof_ladder_level: ProofLadderLevel,
    pub missing_requirements: Vec<LocalMicroFlowProofRequirement>,
    pub dict_v1_representation_only: bool,
    pub failure_action: &'static str,
}

pub fn decide_local_micro_flow_proof_eligibility(
    input: &LocalMicroFlowProofEligibilityInput,
) -> LocalMicroFlowProofEligibilityDecision {
    let missing = input.missing_requirements();
    if missing.is_empty() {
        return LocalMicroFlowProofEligibilityDecision {
            flow_proof_eligible: true,
            packet_status: LocalMicroFlowPacketStatus::MicroFlowFound,
            proof_ladder_level: ProofLadderLevel::FlowProof,
            missing_requirements: missing,
            dict_v1_representation_only: true,
            failure_action: "packet_path_may_claim_flow_proof_after_packet_generation_gate",
        };
    }

    let packet_status =
        if !input.claimed_path_not_truncated || !input.cap_omissions_outside_claimed_segment {
            LocalMicroFlowPacketStatus::MicroFlowTruncated
        } else if !input.backed_by_current_persisted_micro_facts
            || !input.endpoint_facts_current_and_claimable
            || !input.claimable_lifecycle_passport
        {
            LocalMicroFlowPacketStatus::MicroFlowStale
        } else if input.contains_claimable_graph_relation_summary {
            LocalMicroFlowPacketStatus::PartialMicroFlowFound
        } else {
            LocalMicroFlowPacketStatus::NoMicroFlowPathFound
        };

    let proof_ladder_level = if input.contains_claimable_graph_relation_summary {
        ProofLadderLevel::GraphRelationProof
    } else {
        ProofLadderLevel::Unknown
    };

    LocalMicroFlowProofEligibilityDecision {
        flow_proof_eligible: false,
        packet_status,
        proof_ladder_level,
        missing_requirements: missing,
        dict_v1_representation_only: true,
        failure_action: "downgrade_or_omit_flow_proof_and_preserve_explicit_gap_state",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalMicroFlowPacketLinterClass {
    NormalSourceDelta,
    PacketProofIntegrityFinding,
    UnsupportedDegradedState,
}

impl LocalMicroFlowPacketLinterClass {
    pub const fn validation_classification(self) -> ValidationClassification {
        match self {
            Self::NormalSourceDelta => ValidationClassification::Diagnostic,
            Self::PacketProofIntegrityFinding => ValidationClassification::Block,
            Self::UnsupportedDegradedState => ValidationClassification::Degraded,
        }
    }

    pub const fn is_user_source_defect_by_default(self) -> bool {
        false
    }

    pub const fn may_block_packet_proof_availability(self) -> bool {
        matches!(self, Self::PacketProofIntegrityFinding)
    }

    pub const fn may_create_source_code_hard_interrupt_without_reverification(self) -> bool {
        false
    }

    pub const fn recommended_action_kind(self) -> &'static str {
        match self {
            Self::NormalSourceDelta => "safe_to_continue",
            Self::PacketProofIntegrityFinding => "reindex_or_repair_codegraph_state",
            Self::UnsupportedDegradedState => "unsupported_or_unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalMicroFlowSourceDeltaKind {
    AssignmentAdded,
    AssignmentRemoved,
    ReturnAdded,
    BranchAdded,
    PathChanged,
    PacketAddedRemovedOrRekeyed,
}

impl LocalMicroFlowSourceDeltaKind {
    pub const fn linter_class(self) -> LocalMicroFlowPacketLinterClass {
        LocalMicroFlowPacketLinterClass::NormalSourceDelta
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalMicroFlowIntegrityFindingKind {
    PacketReferencesMissingNode,
    PacketReferencesMissingEdge,
    PacketReferencesStaleEdge,
    FlowProofWithUnknownGap,
    FlowProofAfterTruncation,
    BranchIdentityOmitted,
    ReturnPathIdOmitted,
    ShadowedBindingCollapsed,
    PacketMissingSourceSpan,
    PacketMissingProvenance,
    DictV1BodyFailsAuditExpansion,
}

impl LocalMicroFlowIntegrityFindingKind {
    pub const fn linter_class(self) -> LocalMicroFlowPacketLinterClass {
        LocalMicroFlowPacketLinterClass::PacketProofIntegrityFinding
    }

    pub const fn recommended_action(self) -> &'static str {
        "reindex_or_repair_codegraph_packet_state"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalMicroFlowUnsupportedConditionKind {
    LanguageUnsupported,
    DynamicUnresolvedRelation,
    ParserRecovery,
    CapOmission,
    StaleOptionalLayer,
    OldDbWithoutPacketTable,
}

impl LocalMicroFlowUnsupportedConditionKind {
    pub const fn linter_class(self) -> LocalMicroFlowPacketLinterClass {
        LocalMicroFlowPacketLinterClass::UnsupportedDegradedState
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
    fn mvp4_3_packet_status_contract_is_explicit_and_non_activating() {
        let names = LocalMicroFlowPacketStatus::ALL
            .iter()
            .map(|status| status.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "micro_flow_found",
                "partial_micro_flow_found",
                "no_micro_flow_path_found",
                "micro_flow_unavailable",
                "micro_flow_truncated",
                "micro_flow_unsupported",
                "micro_flow_stale",
                "micro_flow_corrupt",
                "micro_flow_incompatible"
            ]
        );
        assert_eq!(
            LocalMicroFlowPacketStatus::from_storage_str("partial_local_flow"),
            Some(LocalMicroFlowPacketStatus::PartialMicroFlowFound)
        );
        assert!(LocalMicroFlowPacketStatus::MicroFlowFound.rows_may_be_claimable());
        assert!(
            LocalMicroFlowPacketStatus::PartialMicroFlowFound.rows_may_be_claimable(),
            "partial rows may claim exact graph-relation segments, not complete flow"
        );
        assert!(!LocalMicroFlowPacketStatus::NoMicroFlowPathFound.rows_may_be_claimable());
        for status in LocalMicroFlowPacketStatus::ALL {
            assert!(status.compact_output_allowed());
            assert!(!status.hard_interrupt_eligible_from_status_only());
            assert!(!status.recommended_action().is_empty());
        }
        assert!(LocalMicroFlowPacketStatus::MicroFlowTruncated.validate_edit_should_warn());
        assert!(!LocalMicroFlowPacketStatus::MicroFlowFound.validate_edit_should_warn());
    }

    #[test]
    fn mvp4_3_complete_local_chain_is_flow_proof_eligible_only_with_all_evidence() {
        let decision = decide_local_micro_flow_proof_eligibility(
            &LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain(),
        );
        assert!(decision.flow_proof_eligible);
        assert_eq!(
            decision.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowFound
        );
        assert_eq!(decision.proof_ladder_level, ProofLadderLevel::FlowProof);
        assert!(decision.missing_requirements.is_empty());
        assert!(decision.dict_v1_representation_only);
        assert_eq!(
            decision.failure_action,
            "packet_path_may_claim_flow_proof_after_packet_generation_gate"
        );
    }

    #[test]
    fn mvp4_3_local_returns_to_only_packet_cannot_claim_flow_proof() {
        let decision = decide_local_micro_flow_proof_eligibility(
            &LocalMicroFlowProofEligibilityInput::local_returns_to_only(),
        );
        assert!(!decision.flow_proof_eligible);
        assert_eq!(
            decision.packet_status,
            LocalMicroFlowPacketStatus::PartialMicroFlowFound
        );
        assert_eq!(
            decision.proof_ladder_level,
            ProofLadderLevel::GraphRelationProof
        );
        assert!(decision
            .missing_requirements
            .contains(&LocalMicroFlowProofRequirement::IncludesLocalFlowsTo));
        assert!(decision
            .missing_requirements
            .contains(&LocalMicroFlowProofRequirement::IncludesRequiredReadsAndWrites));
        assert!(decision
            .missing_requirements
            .contains(&LocalMicroFlowProofRequirement::NotLocalReturnsToOnly));
    }

    #[test]
    fn mvp4_3_missing_read_write_flow_and_lifecycle_downgrade_packet_proof() {
        let mut missing_read_write =
            LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
        missing_read_write.includes_required_reads_and_writes = false;
        let decision = decide_local_micro_flow_proof_eligibility(&missing_read_write);
        assert!(!decision.flow_proof_eligible);
        assert_eq!(
            decision.packet_status,
            LocalMicroFlowPacketStatus::PartialMicroFlowFound
        );
        assert_eq!(
            decision.proof_ladder_level,
            ProofLadderLevel::GraphRelationProof
        );

        let mut stale_fact = LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
        stale_fact.backed_by_current_persisted_micro_facts = false;
        let decision = decide_local_micro_flow_proof_eligibility(&stale_fact);
        assert_eq!(
            decision.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowStale
        );
        assert!(!decision.flow_proof_eligible);

        let mut no_graph_summary =
            LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
        no_graph_summary.contains_claimable_graph_relation_summary = false;
        no_graph_summary.includes_local_flows_to = false;
        no_graph_summary.includes_required_reads_and_writes = false;
        let decision = decide_local_micro_flow_proof_eligibility(&no_graph_summary);
        assert_eq!(
            decision.packet_status,
            LocalMicroFlowPacketStatus::NoMicroFlowPathFound
        );
        assert_eq!(decision.proof_ladder_level, ProofLadderLevel::Unknown);
    }

    #[test]
    fn mvp4_3_branch_return_shadow_cap_recovery_and_dict_v1_boundaries_are_required() {
        for (input, expected) in [
            (
                {
                    let mut input =
                        LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
                    input.branch_identity_preserved = false;
                    input
                },
                LocalMicroFlowProofRequirement::BranchIdentityPreserved,
            ),
            (
                {
                    let mut input =
                        LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
                    input.return_path_identity_preserved = false;
                    input
                },
                LocalMicroFlowProofRequirement::ReturnPathIdentityPreserved,
            ),
            (
                {
                    let mut input =
                        LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
                    input.shadowed_binding_identity_preserved = false;
                    input
                },
                LocalMicroFlowProofRequirement::ShadowedBindingIdentityPreserved,
            ),
            (
                {
                    let mut input =
                        LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
                    input.gaps_outside_claimed_path = false;
                    input
                },
                LocalMicroFlowProofRequirement::GapsOutsideClaimedPath,
            ),
            (
                {
                    let mut input =
                        LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
                    input.dict_v1_lossless_to_audit_ordered_steps = false;
                    input
                },
                LocalMicroFlowProofRequirement::DictV1LosslessToAuditOrderedSteps,
            ),
            (
                {
                    let mut input =
                        LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
                    input.source_spanned_proof_bearing_steps = false;
                    input
                },
                LocalMicroFlowProofRequirement::SourceSpannedProofBearingSteps,
            ),
            (
                {
                    let mut input =
                        LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
                    input.production_safe_source_role = false;
                    input
                },
                LocalMicroFlowProofRequirement::ProductionSafeSourceRole,
            ),
            (
                {
                    let mut input =
                        LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
                    input.no_packet_integrity_finding = false;
                    input
                },
                LocalMicroFlowProofRequirement::NoPacketIntegrityFinding,
            ),
        ] {
            let decision = decide_local_micro_flow_proof_eligibility(&input);
            assert!(
                !decision.flow_proof_eligible,
                "{expected:?} must downgrade flow proof"
            );
            assert!(
                decision.missing_requirements.contains(&expected),
                "missing {expected:?}"
            );
            assert!(decision.dict_v1_representation_only);
        }

        let mut truncated = LocalMicroFlowProofEligibilityInput::complete_local_assignment_chain();
        truncated.claimed_path_not_truncated = false;
        let decision = decide_local_micro_flow_proof_eligibility(&truncated);
        assert_eq!(
            decision.packet_status,
            LocalMicroFlowPacketStatus::MicroFlowTruncated
        );
        assert!(!decision.flow_proof_eligible);
    }

    #[test]
    fn mvp4_3_packet_linter_contract_separates_source_deltas_from_tool_integrity() {
        let normal = LocalMicroFlowSourceDeltaKind::AssignmentAdded.linter_class();
        assert_eq!(normal, LocalMicroFlowPacketLinterClass::NormalSourceDelta);
        assert_eq!(
            normal.validation_classification(),
            ValidationClassification::Diagnostic
        );
        assert_eq!(normal.recommended_action_kind(), "safe_to_continue");
        assert!(!normal.is_user_source_defect_by_default());
        assert!(!normal.may_create_source_code_hard_interrupt_without_reverification());

        let integrity = LocalMicroFlowIntegrityFindingKind::FlowProofWithUnknownGap;
        assert_eq!(
            integrity.linter_class(),
            LocalMicroFlowPacketLinterClass::PacketProofIntegrityFinding
        );
        assert_eq!(
            integrity.linter_class().validation_classification(),
            ValidationClassification::Block
        );
        assert!(integrity
            .linter_class()
            .may_block_packet_proof_availability());
        assert_eq!(
            integrity.recommended_action(),
            "reindex_or_repair_codegraph_packet_state"
        );
        assert!(!integrity
            .linter_class()
            .may_create_source_code_hard_interrupt_without_reverification());

        let unsupported = LocalMicroFlowUnsupportedConditionKind::CapOmission.linter_class();
        assert_eq!(
            unsupported,
            LocalMicroFlowPacketLinterClass::UnsupportedDegradedState
        );
        assert_eq!(
            unsupported.validation_classification(),
            ValidationClassification::Degraded
        );
        assert_eq!(
            unsupported.recommended_action_kind(),
            "unsupported_or_unknown"
        );
        assert!(!unsupported.may_create_source_code_hard_interrupt_without_reverification());
    }

    #[test]
    fn mvp4_3_first_slice_scope_is_typescript_function_local_and_non_mutation() {
        assert_eq!(
            MVP4_3_LOCAL_MICRO_FLOW_PACKET_KIND,
            "function_local_micro_flow_packet"
        );
        assert_eq!(MVP4_3_LOCAL_MICRO_FLOW_PACKET_ENCODING, "dict_v1");
        assert_eq!(
            MVP4_3_LOCAL_MICRO_FLOW_PACKET_EXTRACTION_VERSION,
            "mvp4.3-typescript-local-micro-flow-packets-v1"
        );
        assert_eq!(
            MVP4_3_TYPESCRIPT_FIRST_SLICE_RELATIONS,
            &[
                MicroEdgeKind::LocalReads,
                MicroEdgeKind::LocalWrites,
                MicroEdgeKind::LocalFlowsTo,
                MicroEdgeKind::LocalReturnsTo
            ]
        );
        assert!(!MVP4_3_TYPESCRIPT_FIRST_SLICE_RELATIONS.contains(&MicroEdgeKind::LocalMutates));
        assert!(!MVP4_3_TYPESCRIPT_FIRST_SLICE_RELATIONS.contains(&MicroEdgeKind::LocalCalls));
    }

    #[test]
    fn micro_edge_language_capability_registry_defaults_to_not_implemented() {
        let active = MVP4_ACTIVE_MICRO_EDGE_LANGUAGE_CAPABILITIES;
        // LOCAL_RETURNS_TO (MVP4.2) plus LOCAL_READS / LOCAL_WRITES (exact) and
        // LOCAL_FLOWS_TO (derived_with_provenance) (MVP4.2b).
        assert_eq!(active.len(), 4);
        assert!(active.iter().all(|capability| {
            capability.language == "typescript"
                && matches!(
                    capability.activation_status,
                    MicroEdgeSupportStatus::ExactCapable
                        | MicroEdgeSupportStatus::DerivedWithProvenanceCapable
                )
        }));

        let returns_to =
            mvp4_micro_edge_language_capability("typescript", MicroEdgeKind::LocalReturnsTo);
        assert_eq!(returns_to.micro_edge_kind, MicroEdgeKind::LocalReturnsTo);
        assert_eq!(
            returns_to.endpoint_node_requirements,
            &[MicroNodeKind::ReturnSite, MicroNodeKind::FunctionFrame]
        );
        assert!(returns_to.supports_claimable_exact(
            "tree-sitter-typescript",
            MicroSourceRole::Production,
            MicroExactness::Exact,
            MVP4_2_LOCAL_RETURNS_TO_CLAIMABILITY,
            MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION,
        ));

        let reads = mvp4_micro_edge_language_capability("typescript", MicroEdgeKind::LocalReads);
        assert_eq!(
            reads.activation_status,
            MicroEdgeSupportStatus::ExactCapable
        );
        assert!(reads.supports_claimable_exact(
            "tree-sitter-typescript",
            MicroSourceRole::Production,
            MicroExactness::Exact,
            MVP4_2B_LOCAL_READS_CLAIMABILITY,
            MVP4_2B_LOCAL_READS_EXTRACTION_VERSION,
        ));

        let writes = mvp4_micro_edge_language_capability("typescript", MicroEdgeKind::LocalWrites);
        assert_eq!(
            writes.activation_status,
            MicroEdgeSupportStatus::ExactCapable
        );
        assert!(writes.supports_claimable_exact(
            "tree-sitter-typescript",
            MicroSourceRole::Production,
            MicroExactness::Exact,
            MVP4_2B_LOCAL_WRITES_CLAIMABILITY,
            MVP4_2B_LOCAL_WRITES_EXTRACTION_VERSION,
        ));

        // LOCAL_FLOWS_TO is DERIVED, not exact: it must NOT report exact support,
        // and carries derived_with_provenance exactness + binding endpoints.
        let flows_to =
            mvp4_micro_edge_language_capability("typescript", MicroEdgeKind::LocalFlowsTo);
        assert_eq!(
            flows_to.activation_status,
            MicroEdgeSupportStatus::DerivedWithProvenanceCapable
        );
        assert_eq!(
            flows_to.exactness_capability,
            MicroExactness::DerivedWithProvenance
        );
        assert_eq!(
            flows_to.endpoint_node_requirements,
            &[MicroNodeKind::Parameter, MicroNodeKind::LocalBinding]
        );
        assert_eq!(
            flows_to.claimability_label,
            Some(MVP4_2B_LOCAL_FLOWS_TO_CLAIMABILITY)
        );
        assert_eq!(
            flows_to.extraction_version,
            Some(MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION)
        );
        assert!(!flows_to.activation_status.default_exact_support());
        assert!(!flows_to.supports_claimable_exact(
            "tree-sitter-typescript",
            MicroSourceRole::Production,
            MicroExactness::DerivedWithProvenance,
            MVP4_2B_LOCAL_FLOWS_TO_CLAIMABILITY,
            MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION,
        ));

        // Other languages do not inherit TypeScript exact support for any kind.
        for language in ["javascript", "rust", "python", "go", "java"] {
            for kind in [
                MicroEdgeKind::LocalReturnsTo,
                MicroEdgeKind::LocalReads,
                MicroEdgeKind::LocalWrites,
                MicroEdgeKind::LocalFlowsTo,
            ] {
                let capability = mvp4_micro_edge_language_capability(language, kind);
                assert_eq!(
                    capability.activation_status,
                    MicroEdgeSupportStatus::NotImplemented,
                    "{language}/{kind:?} should not inherit TypeScript exact support"
                );
                assert!(!capability.activation_status.default_exact_support());
            }
        }
    }

    #[test]
    fn micro_edge_identity_keeps_language_and_frontend_domains_distinct() {
        let fixture = local_returns_to_identity_fixture();
        let typescript = local_returns_to_identity_input(&fixture);
        let mut javascript_fixture = fixture.clone();
        javascript_fixture.language = "javascript@tree-sitter-javascript".to_string();
        let javascript = local_returns_to_identity_input(&javascript_fixture);

        assert_ne!(
            stable_micro_edge_id(&typescript),
            stable_micro_edge_id(&javascript),
            "same path/span/endpoints must stay distinct across language adapters"
        );

        let mut alternate_frontend = fixture;
        alternate_frontend.language = "typescript@future-typescript-frontend".to_string();
        let alternate_frontend = local_returns_to_identity_input(&alternate_frontend);
        assert_ne!(
            stable_micro_edge_id(&typescript),
            stable_micro_edge_id(&alternate_frontend),
            "frontend identity participates in the language domain"
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
