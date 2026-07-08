use serde::{Deserialize, Serialize};

pub const DIRTY_EVIDENCE_REGISTRY_SCHEMA_VERSION: u32 = 1;
pub const PROOF_LADDER_INVALIDATION_CONTRACT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirtyEvidenceKind {
    GraphEntities,
    GraphEdges,
    SourceSpans,
    SourceRoles,
    TextEvidence,
    FileManifest,
    LanguageCapabilityTags,
    LanguageSourceRoleTags,
    ParserFactBundle,
    ResolverMetadata,
    ProjectConfigMetadata,
    CompilerLspMetadata,
    UnresolvedReference,
    PathEvidence,
    ProofPathCache,
    GraphDeltaSnapshot,
    ValidationFinding,
    CandidateSpoolPacket,
    CandidateSpoolQueryIndexRow,
    RuntimeVectorChunk,
    VectorRuntimeSidecarManifest,
    VectorAuditArtifact,
    BinaryCandidateRecord,
    NuanceTokenRecord,
    SourceNavigationHandle,
    RoutingPacketHandle,
    ContextPacketHandle,
    InvestigationPacketHandle,
    ValidateEditPacket,
    ValidationPacket,
    HardInterruptPacket,
    SeveritySummary,
    ContextPackPacket,
    StatusDoctorPacket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofLadderLevel {
    TextEvidence,
    SymbolEvidence,
    CandidateEvidence,
    SourceNavigationEvidence,
    GraphRelationProof,
    MutationProof,
    FlowProof,
    Unknown,
    Unsupported,
    DiagnosticOnly,
}

impl ProofLadderLevel {
    pub const ALL: &'static [Self] = &[
        Self::TextEvidence,
        Self::SymbolEvidence,
        Self::CandidateEvidence,
        Self::SourceNavigationEvidence,
        Self::GraphRelationProof,
        Self::MutationProof,
        Self::FlowProof,
        Self::Unknown,
        Self::Unsupported,
        Self::DiagnosticOnly,
    ];

    pub const fn is_graph_proof_level(self) -> bool {
        matches!(
            self,
            Self::GraphRelationProof | Self::MutationProof | Self::FlowProof
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirtyEvidenceFreshnessState {
    Fresh,
    Stale,
    Dirty,
    Missing,
    Corrupt,
    Inaccessible,
    PermissionDenied,
    NotApplicable,
    Rebuilding,
    Publishing,
    Partial,
    Truncated,
    DiagnosticOnly,
    Unknown,
}

impl DirtyEvidenceFreshnessState {
    pub const ALL: &'static [Self] = &[
        Self::Fresh,
        Self::Stale,
        Self::Dirty,
        Self::Missing,
        Self::Corrupt,
        Self::Inaccessible,
        Self::PermissionDenied,
        Self::NotApplicable,
        Self::Rebuilding,
        Self::Publishing,
        Self::Partial,
        Self::Truncated,
        Self::DiagnosticOnly,
        Self::Unknown,
    ];

    pub const fn is_fresh(self) -> bool {
        matches!(self, Self::Fresh)
    }

    pub const fn is_unsafe_for_graph_proof(self) -> bool {
        matches!(
            self,
            Self::Stale
                | Self::Dirty
                | Self::Missing
                | Self::Corrupt
                | Self::Inaccessible
                | Self::PermissionDenied
                | Self::Rebuilding
                | Self::Publishing
                | Self::Partial
                | Self::Truncated
                | Self::DiagnosticOnly
                | Self::Unknown
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirtyEvidenceBinding {
    Required,
    Optional,
    NotApplicable,
}

impl DirtyEvidenceBinding {
    pub const fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirtyEvidenceStorageLayer {
    GraphDb,
    GraphStore,
    Sidecar,
    Packet,
    Lifecycle,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirtyEvidenceClaimabilityEffect {
    GraphProofAvailable,
    SupportsGraphProofWhenJoined,
    GraphProofUnavailable,
    NonProofEvidenceOnly,
    SidecarOnly,
    DiagnosticOnly,
    NoGraphDbEffect,
    ConditionalFutureProofOnly,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirtyEvidenceRefreshStrategy {
    ReindexChangedFile,
    ReindexDirtyClosure,
    RecomputeSourceSpans,
    RefreshSourceRoleTags,
    RefreshTextEvidence,
    InvalidateAndReverifyPathEvidence,
    RebuildCandidateSpool,
    RebuildCandidateQueryIndex,
    RebuildVectorRuntimeSidecar,
    RebuildVectorAuditArtifact,
    RegeneratePacket,
    StatusOnly,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyEvidenceRegistryEntry {
    pub surface_name: String,
    pub evidence_kind: DirtyEvidenceKind,
    pub proof_ladder_level: ProofLadderLevel,
    pub graph_proof_possible: bool,
    pub claimability_effect_when_fresh: DirtyEvidenceClaimabilityEffect,
    pub claimability_effect_when_stale: DirtyEvidenceClaimabilityEffect,
    pub source_binding: DirtyEvidenceBinding,
    pub file_binding: DirtyEvidenceBinding,
    pub span_binding: DirtyEvidenceBinding,
    pub entity_binding: DirtyEvidenceBinding,
    pub edge_binding: DirtyEvidenceBinding,
    pub sidecar_binding: DirtyEvidenceBinding,
    pub lifecycle_binding: DirtyEvidenceBinding,
    pub storage_layer: DirtyEvidenceStorageLayer,
    pub invalidated_by: Vec<String>,
    pub refresh_strategy: DirtyEvidenceRefreshStrategy,
    pub stale_behavior_in_query: String,
    pub stale_behavior_in_context_pack: String,
    pub stale_behavior_in_validate_edit: String,
    pub stale_behavior_in_mcp: String,
    pub status_behavior: String,
    pub corrupt_behavior: String,
    pub inaccessible_behavior: String,
    pub not_applicable_behavior: String,
}

impl DirtyEvidenceRegistryEntry {
    pub fn claimability_effect_for_state(
        &self,
        state: DirtyEvidenceFreshnessState,
    ) -> DirtyEvidenceClaimabilityEffect {
        match state {
            DirtyEvidenceFreshnessState::Fresh => self.claimability_effect_when_fresh,
            DirtyEvidenceFreshnessState::NotApplicable => {
                DirtyEvidenceClaimabilityEffect::NotApplicable
            }
            DirtyEvidenceFreshnessState::DiagnosticOnly => {
                DirtyEvidenceClaimabilityEffect::DiagnosticOnly
            }
            DirtyEvidenceFreshnessState::Missing
                if matches!(self.storage_layer, DirtyEvidenceStorageLayer::Sidecar) =>
            {
                DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
            }
            DirtyEvidenceFreshnessState::Corrupt
                if matches!(self.storage_layer, DirtyEvidenceStorageLayer::Sidecar) =>
            {
                DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
            }
            DirtyEvidenceFreshnessState::Inaccessible
            | DirtyEvidenceFreshnessState::PermissionDenied
                if matches!(self.storage_layer, DirtyEvidenceStorageLayer::Sidecar) =>
            {
                DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
            }
            _ => self.claimability_effect_when_stale,
        }
    }

    pub fn can_support_graph_proof_in_state(&self, state: DirtyEvidenceFreshnessState) -> bool {
        state.is_fresh()
            && self.graph_proof_possible
            && !matches!(
                self.claimability_effect_when_fresh,
                DirtyEvidenceClaimabilityEffect::NonProofEvidenceOnly
                    | DirtyEvidenceClaimabilityEffect::SidecarOnly
                    | DirtyEvidenceClaimabilityEffect::DiagnosticOnly
                    | DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
                    | DirtyEvidenceClaimabilityEffect::ConditionalFutureProofOnly
                    | DirtyEvidenceClaimabilityEffect::NotApplicable
            )
    }

    pub fn can_claim_graph_relation_proof_in_state(
        &self,
        state: DirtyEvidenceFreshnessState,
    ) -> bool {
        self.can_support_graph_proof_in_state(state)
            && matches!(
                self.proof_ladder_level,
                ProofLadderLevel::GraphRelationProof
            )
    }

    pub fn state_blocks_graph_db_claimability(&self, state: DirtyEvidenceFreshnessState) -> bool {
        if !state.is_unsafe_for_graph_proof()
            || matches!(state, DirtyEvidenceFreshnessState::NotApplicable)
        {
            return false;
        }
        matches!(
            self.storage_layer,
            DirtyEvidenceStorageLayer::GraphDb
                | DirtyEvidenceStorageLayer::GraphStore
                | DirtyEvidenceStorageLayer::Lifecycle
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofLadderInvalidationRule {
    pub proof_ladder_level: ProofLadderLevel,
    pub fresh_claim: String,
    pub cannot_claim: Vec<String>,
    pub changed_file_invalidates: Vec<String>,
    pub stale_can_block_claimable_graph_proof: bool,
    pub stale_severity_effect: String,
    pub stale_hard_interrupt_eligible: bool,
    pub output_change_label: String,
    pub fresh_claimability_effect: DirtyEvidenceClaimabilityEffect,
    pub stale_claimability_effect: DirtyEvidenceClaimabilityEffect,
}

pub fn dirty_evidence_registry() -> Vec<DirtyEvidenceRegistryEntry> {
    use DirtyEvidenceBinding::{NotApplicable as NA, Optional as Opt, Required as Req};
    use DirtyEvidenceClaimabilityEffect::{
        ConditionalFutureProofOnly as Future, DiagnosticOnly, GraphProofAvailable,
        GraphProofUnavailable, NoGraphDbEffect, NonProofEvidenceOnly, SidecarOnly,
        SupportsGraphProofWhenJoined,
    };
    use DirtyEvidenceFreshnessState::Stale;
    use DirtyEvidenceKind::*;
    use DirtyEvidenceRefreshStrategy::*;
    use DirtyEvidenceStorageLayer::*;
    use ProofLadderLevel::{
        CandidateEvidence as LevelCandidateEvidence, DiagnosticOnly as LevelDiagnosticOnly,
        FlowProof as LevelFlowProof, GraphRelationProof as LevelGraphRelationProof,
        MutationProof as LevelMutationProof, SourceNavigationEvidence as LevelSourceNavigation,
        SymbolEvidence as LevelSymbolEvidence, TextEvidence as LevelTextEvidence,
    };

    vec![
        entry(
            "graph_entities",
            GraphEntities,
            LevelSymbolEvidence,
            true,
            GraphProofAvailable,
            GraphProofUnavailable,
            Req,
            Req,
            Req,
            Req,
            NA,
            NA,
            Req,
            GraphDb,
            &["changed_file_content", "deleted_file", "renamed_file", "scope_policy_changed"],
            ReindexChangedFile,
            graph_stale_query(),
            graph_stale_context_pack(),
            graph_stale_validate_edit(),
            graph_stale_mcp(),
            "reported as graph entity freshness; unsafe state makes graph proof unavailable",
            graph_corrupt_behavior(),
            graph_inaccessible_behavior(),
            "not applicable only for unsupported languages/files outside indexed scope",
        ),
        entry(
            "graph_edges",
            GraphEdges,
            LevelGraphRelationProof,
            true,
            GraphProofAvailable,
            GraphProofUnavailable,
            Req,
            Req,
            Req,
            Req,
            Req,
            NA,
            Req,
            GraphDb,
            &["changed_file_content", "deleted_file", "renamed_file", "source_endpoint_changed"],
            ReindexChangedFile,
            graph_stale_query(),
            graph_stale_context_pack(),
            graph_stale_validate_edit(),
            graph_stale_mcp(),
            "reported as graph edge freshness; unsafe state makes graph relation proof unavailable",
            graph_corrupt_behavior(),
            graph_inaccessible_behavior(),
            "not applicable only when the relation family is unsupported or out of scope",
        ),
        entry(
            "source_spans",
            SourceSpans,
            LevelSymbolEvidence,
            true,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Req,
            Opt,
            Opt,
            NA,
            Req,
            GraphStore,
            &["changed_file_content", "deleted_file", "renamed_file", "parser_span_shift"],
            RecomputeSourceSpans,
            graph_stale_query(),
            graph_stale_context_pack(),
            graph_stale_validate_edit(),
            graph_stale_mcp(),
            "reported as source-span freshness; required source spans gate claimable graph proof",
            graph_corrupt_behavior(),
            graph_inaccessible_behavior(),
            "not applicable for synthetic diagnostics without a source span",
        ),
        entry(
            "source_roles",
            SourceRoles,
            LevelSymbolEvidence,
            false,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Opt,
            Opt,
            Opt,
            NA,
            Req,
            GraphStore,
            &["changed_file_content", "path_classification_changed", "scope_policy_changed"],
            RefreshSourceRoleTags,
            "stale role evidence is omitted from production proof filters until refreshed",
            "stale role evidence is labeled and cannot make test/mock evidence production proof",
            "stale role evidence downgrades affected validation output to unknown or diagnostic",
            "stale role evidence is emitted as lifecycle/evidence warning, not graph proof",
            "reported as source-role freshness and proof-eligibility metadata",
            "source-role store corruption is graph-store metadata corruption, not sidecar corruption",
            graph_inaccessible_behavior(),
            "not applicable for files outside indexed scope",
        ),
        entry(
            "text_evidence",
            TextEvidence,
            LevelTextEvidence,
            false,
            NonProofEvidenceOnly,
            NoGraphDbEffect,
            Req,
            Req,
            Opt,
            NA,
            NA,
            NA,
            Req,
            GraphStore,
            &["changed_file_content", "deleted_file", "renamed_file", "text_extraction_changed"],
            RefreshTextEvidence,
            "stale text evidence is not returned as fresh source-text evidence",
            "stale text evidence may appear only as stale/no-proof fallback, not graph proof",
            "stale text evidence means source-text evidence unavailable or stale, not graph breakage",
            "stale text evidence is labeled non-proof in MCP output",
            "reported as source-text freshness with claimable_for_graph=false",
            "text evidence corruption is text evidence corruption; it is not a broken graph relation",
            "text evidence inaccessible means source-text fallback unavailable",
            "not applicable for binary/unsupported files with no text lane",
        ),
        entry(
            "file_manifest",
            FileManifest,
            LevelDiagnosticOnly,
            false,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            NA,
            NA,
            NA,
            NA,
            Req,
            GraphDb,
            &["changed_file_content", "deleted_file", "renamed_file", "scope_policy_changed"],
            ReindexChangedFile,
            graph_stale_query(),
            graph_stale_context_pack(),
            graph_stale_validate_edit(),
            graph_stale_mcp(),
            "reported as DB file-manifest lifecycle; stale manifests make DB graph proof unsafe",
            graph_corrupt_behavior(),
            graph_inaccessible_behavior(),
            "not applicable only when no graph DB is expected",
        ),
        entry(
            "language_capability_tags",
            LanguageCapabilityTags,
            LevelDiagnosticOnly,
            false,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Opt,
            Opt,
            Opt,
            NA,
            Req,
            GraphStore,
            &[
                "changed_file_content",
                "language_detection_changed",
                "capability_registry_changed",
                "resolver_status_changed",
            ],
            RefreshSourceRoleTags,
            "stale capability tags are not used to claim exact parser/resolver support",
            "stale capability tags are labeled and cannot upgrade fallback evidence to proof",
            "stale capability tags make exact validation eligibility unavailable until refreshed",
            "stale capability tags are emitted as capability metadata freshness, not proof",
            "reported as language capability freshness; tier labels alone never authorize proof",
            "capability tag corruption is graph-store metadata corruption",
            graph_inaccessible_behavior(),
            "not applicable for unsupported files outside the registered frontend matrix",
        ),
        entry(
            "language_source_role_tags",
            LanguageSourceRoleTags,
            LevelDiagnosticOnly,
            false,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Opt,
            Opt,
            NA,
            NA,
            Req,
            GraphStore,
            &["changed_file_content", "path_classification_changed", "language_detection_changed"],
            RefreshSourceRoleTags,
            "stale language/source-role tags are omitted from exact proof eligibility",
            "stale language/source-role tags are labeled and bounded",
            "stale tags cannot be used to certify a production validation boundary",
            "stale tags are emitted as diagnostic freshness labels",
            "reported as source-role/language coverage freshness",
            "tag corruption is graph-store metadata corruption",
            graph_inaccessible_behavior(),
            "not applicable for unsupported files outside the indexed language matrix",
        ),
        entry(
            "parser_fact_bundles",
            ParserFactBundle,
            LevelSymbolEvidence,
            true,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Req,
            Req,
            Opt,
            NA,
            Req,
            GraphStore,
            &[
                "changed_file_content",
                "deleted_file",
                "renamed_file",
                "parser_error_changed",
                "parser_version_changed",
            ],
            ReindexChangedFile,
            "stale parser facts are not returned as claimable parser/source-span facts",
            "stale parser facts are omitted or labeled parser_stale/no_proof until reindexed",
            "stale parser facts cannot block validation as current source proof",
            "stale parser facts are emitted as non-claimable parser freshness in MCP",
            "reported as parser fact bundle freshness; parser facts remain separate from resolver/compiler proof",
            "parser fact corruption is graph-store parser metadata corruption",
            graph_inaccessible_behavior(),
            "not applicable when the registered frontend cannot parse the file",
        ),
        entry(
            "resolver_metadata",
            ResolverMetadata,
            LevelGraphRelationProof,
            true,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Opt,
            Req,
            Req,
            NA,
            Req,
            GraphStore,
            &[
                "changed_file_content",
                "deleted_file",
                "renamed_file",
                "project_config_changed",
                "resolver_version_changed",
                "dependency_metadata_changed",
            ],
            ReindexDirtyClosure,
            "stale resolver metadata is not returned as compiler/resolver exactness",
            "stale resolver metadata downgrades exact graph context to unknown/no-proof",
            "stale resolver metadata cannot authorize blockers without fresh provenance",
            "stale resolver metadata is emitted with reindex/repair recovery in MCP",
            "reported as resolver provenance/currentness metadata where semantic exactness is claimed",
            "resolver metadata corruption is graph-store resolver metadata corruption",
            graph_inaccessible_behavior(),
            "not applicable when the language resolver is not implemented or did not run",
        ),
        entry(
            "project_config_metadata",
            ProjectConfigMetadata,
            LevelDiagnosticOnly,
            false,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            NA,
            Opt,
            Opt,
            NA,
            Req,
            GraphStore,
            &[
                "project_config_changed",
                "dependency_metadata_changed",
                "build_profile_changed",
                "workspace_root_changed",
            ],
            ReindexDirtyClosure,
            "stale project config metadata is not used for module/package exactness",
            "stale project config metadata downgrades config-dependent context to compiler_required or unknown",
            "stale project config metadata cannot create source-code findings; recommend reindex/repair",
            "stale project config metadata is emitted as tool/project state in MCP",
            "reported as project configuration currentness for resolver/build-aware facts",
            "project config metadata corruption is tool metadata corruption, not source proof",
            "project config metadata inaccessible makes config-dependent proof unavailable",
            "not applicable for languages or repos without supported project config discovery",
        ),
        entry(
            "compiler_lsp_metadata",
            CompilerLspMetadata,
            LevelGraphRelationProof,
            true,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Opt,
            Req,
            Req,
            NA,
            Req,
            GraphStore,
            &[
                "changed_file_content",
                "project_config_changed",
                "compiler_diagnostics_changed",
                "lsp_session_changed",
                "resolver_version_changed",
            ],
            ReindexDirtyClosure,
            "stale compiler/LSP metadata is not returned as semantic proof",
            "stale compiler/LSP metadata downgrades exact semantic context to compiler_required/lsp_required",
            "stale compiler/LSP metadata cannot block validation without fresh provenance",
            "stale compiler/LSP metadata is emitted as resolver/tool-state freshness in MCP",
            "reported as compiler/LSP provenance when such semantic exactness is claimed",
            "compiler/LSP metadata corruption is tool metadata corruption, not source proof",
            "compiler/LSP metadata inaccessible makes semantic proof unavailable",
            "not applicable when compiler/LSP integration did not run for the language",
        ),
        entry(
            "unresolved_references",
            UnresolvedReference,
            LevelDiagnosticOnly,
            false,
            NonProofEvidenceOnly,
            NoGraphDbEffect,
            Req,
            Req,
            Req,
            Opt,
            NA,
            NA,
            Req,
            GraphStore,
            &[
                "changed_file_content",
                "deleted_file",
                "renamed_file",
                "resolver_metadata_changed",
                "source_role_changed",
            ],
            ReindexChangedFile,
            "stale unresolved-reference rows are not returned as current hallucination diagnostics",
            "stale unresolved-reference rows are omitted or labeled non-proof diagnostic context",
            "stale unresolved-reference rows cannot hard-interrupt without fresh eligible policy proof",
            "stale unresolved-reference rows are emitted as diagnostic freshness in MCP",
            "reported as unresolved-reference classifier freshness; unresolved rows are not graph proof",
            "unresolved-reference metadata corruption is diagnostic/index metadata corruption",
            "unresolved-reference metadata inaccessible means the classifier lane is unavailable",
            "not applicable when unresolved-reference extraction is unsupported for the language/relation",
        ),
        entry(
            "PathEvidence",
            PathEvidence,
            LevelGraphRelationProof,
            true,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Req,
            Req,
            Req,
            NA,
            Req,
            GraphStore,
            &["changed_file_content", "deleted_file", "renamed_file", "edge_endpoint_changed"],
            InvalidateAndReverifyPathEvidence,
            "stale PathEvidence is not used as proof; query must reverify graph/source path first",
            "stale PathEvidence is omitted or labeled bounded/no-proof until reverified",
            "stale PathEvidence cannot justify blocking validation until graph/source reverified",
            "stale PathEvidence is labeled non-claimable for graph proof in MCP",
            "reported as path-evidence freshness; fresh cache still requires graph/source verification before proof use",
            "PathEvidence corruption is graph-store path cache corruption",
            "PathEvidence inaccessible means cached path proof support unavailable",
            "not applicable when no path cache exists for the changed surface",
        ),
        entry(
            "proof_path_caches",
            ProofPathCache,
            LevelGraphRelationProof,
            true,
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
            Req,
            Req,
            Req,
            Req,
            Req,
            Opt,
            Req,
            GraphStore,
            &["changed_file_content", "deleted_file", "renamed_file", "proof_path_dependency_changed"],
            InvalidateAndReverifyPathEvidence,
            "stale proof path cache is not returned as proof without revalidation",
            "stale proof path cache is labeled bounded/no-proof",
            "stale proof path cache cannot create a hard interrupt",
            "stale proof path cache is labeled non-claimable in MCP output",
            "reported as optional proof-path cache freshness",
            "proof-path cache corruption does not imply source code is broken",
            "proof-path cache inaccessible means proof support must be recomputed",
            "not applicable when no persisted proof path cache is configured",
        ),
        entry(
            "graph_delta_snapshots",
            GraphDeltaSnapshot,
            LevelDiagnosticOnly,
            false,
            DiagnosticOnly,
            NoGraphDbEffect,
            Req,
            Req,
            Opt,
            Opt,
            Opt,
            NA,
            Req,
            Diagnostic,
            &["changed_file_content", "delta_state_superseded", "update_attempt_failed"],
            RegeneratePacket,
            "stale delta snapshots are diagnostic-only and not used as current graph proof",
            "stale delta snapshots are omitted or labeled stale",
            "stale delta snapshots cannot validate the edit as current",
            "stale delta snapshots are labeled diagnostic-only in MCP",
            "reported as update/delta diagnostic freshness",
            "delta snapshot corruption is diagnostic artifact corruption",
            "delta snapshot inaccessible means historical delta details unavailable",
            "not applicable when no persisted delta snapshot exists",
        ),
        entry(
            "validation_findings",
            ValidationFinding,
            LevelDiagnosticOnly,
            false,
            DiagnosticOnly,
            NoGraphDbEffect,
            Req,
            Req,
            Opt,
            Opt,
            Opt,
            NA,
            Req,
            Diagnostic,
            &["changed_file_content", "validation_rerun", "lifecycle_changed"],
            RegeneratePacket,
            "stale findings are prior diagnostics and cannot be current proof",
            "stale findings are omitted or labeled historical",
            "stale findings cannot block the current edit without rerun",
            "stale findings are labeled diagnostic-only in MCP",
            "reported as validation finding freshness",
            "finding artifact corruption is diagnostic artifact corruption",
            "findings inaccessible means previous validation details unavailable",
            "not applicable if validation findings are not persisted",
        ),
        sidecar_entry(
            "candidate_spool_packets",
            CandidateSpoolPacket,
            LevelCandidateEvidence,
            RebuildCandidateSpool,
            &["changed_file_content", "deleted_file", "renamed_file", "candidate_source_binding_changed"],
            "candidate spool packets are candidate evidence only; stale packets are not returned as fresh",
        ),
        sidecar_entry(
            "candidate_spool_query_index_rows",
            CandidateSpoolQueryIndexRow,
            LevelCandidateEvidence,
            RebuildCandidateQueryIndex,
            &["changed_file_content", "deleted_file", "renamed_file", "candidate_spool_rebuilt", "query_index_corrupt"],
            "candidate spool query-index corruption/unavailability is sidecar/index state, not graph DB corruption",
        ),
        sidecar_entry(
            "runtime_vector_chunks",
            RuntimeVectorChunk,
            LevelCandidateEvidence,
            RebuildVectorRuntimeSidecar,
            &["changed_file_content", "deleted_file", "renamed_file", "embedding_scope_changed", "embedding_profile_changed"],
            "runtime vector chunks are candidate evidence only; stale chunks are not graph proof",
        ),
        sidecar_entry(
            "vector_runtime_sidecar_manifest",
            VectorRuntimeSidecarManifest,
            LevelCandidateEvidence,
            RebuildVectorRuntimeSidecar,
            &["changed_file_content", "scope_policy_changed", "embedding_profile_changed", "manifest_scope_changed"],
            "vector runtime manifest staleness makes vector candidates unavailable/stale only",
        ),
        sidecar_entry(
            "vector_audit_artifact",
            VectorAuditArtifact,
            LevelDiagnosticOnly,
            RebuildVectorAuditArtifact,
            &["vector_runtime_rebuilt", "audit_artifact_missing", "scope_policy_changed"],
            "vector audit artifacts are diagnostics and cannot be graph proof",
        ),
        sidecar_entry(
            "binary_candidate_records",
            BinaryCandidateRecord,
            LevelCandidateEvidence,
            RebuildCandidateSpool,
            &["changed_file_content", "deleted_file", "renamed_file", "binary_signature_changed"],
            "binary candidate records are optional candidate evidence and may be not_applicable",
        ),
        sidecar_entry(
            "nuance_token_records",
            NuanceTokenRecord,
            LevelCandidateEvidence,
            RebuildCandidateSpool,
            &["changed_file_content", "deleted_file", "renamed_file", "nuance_token_changed"],
            "nuance token records are optional candidate evidence and may be not_applicable",
        ),
        sidecar_entry(
            "source_navigation_handles",
            SourceNavigationHandle,
            LevelSourceNavigation,
            RegeneratePacket,
            &["changed_file_content", "deleted_file", "renamed_file", "span_shift", "source_navigation_dependency_changed"],
            "source-navigation handles guide inspection only; stale handles are not relation proof",
        ),
        sidecar_entry(
            "routing_packet_handles",
            RoutingPacketHandle,
            LevelSourceNavigation,
            RegeneratePacket,
            &["changed_file_content", "candidate_layer_changed", "vector_layer_changed", "source_navigation_changed"],
            "routing handles are packet references and cannot become graph proof",
        ),
        sidecar_entry(
            "context_packet_handles",
            ContextPacketHandle,
            LevelSourceNavigation,
            RegeneratePacket,
            &["changed_file_content", "candidate_layer_changed", "vector_layer_changed", "source_navigation_changed"],
            "context packet handles are packet references and cannot become graph proof",
        ),
        sidecar_entry(
            "investigation_packet_handles",
            InvestigationPacketHandle,
            LevelSourceNavigation,
            RegeneratePacket,
            &["changed_file_content", "source_navigation_changed", "artifact_dependency_changed"],
            "investigation handles are source-navigation support and may be not_applicable",
        ),
        packet_entry(
            "validate_edit_packet",
            ValidateEditPacket,
            LevelDiagnosticOnly,
            &["validation_rerun", "changed_file_content", "lifecycle_changed"],
            "validate-edit packets report proof status; the packet itself is not graph proof",
        ),
        packet_entry(
            "validation_packet",
            ValidationPacket,
            LevelDiagnosticOnly,
            &["validation_rerun", "changed_file_content", "lifecycle_changed"],
            "validation packets carry current proof decisions but are not proof without graph/source evidence",
        ),
        packet_entry(
            "hard_interrupt_packet",
            HardInterruptPacket,
            LevelDiagnosticOnly,
            &["validation_rerun", "hard_interrupt_eligibility_changed", "lifecycle_changed"],
            "hard interrupt packets require fresh eligible graph/source proof; stale packets cannot interrupt",
        ),
        packet_entry(
            "severity_summary",
            SeveritySummary,
            LevelDiagnosticOnly,
            &["validation_rerun", "severity_policy_changed", "lifecycle_changed"],
            "severity labels interpret validation output; severity is not graph proof",
        ),
        packet_entry(
            "context_pack_packet",
            ContextPackPacket,
            LevelDiagnosticOnly,
            &["context_pack_rerun", "changed_file_content", "candidate_layer_changed", "path_evidence_changed"],
            "context-pack packets aggregate evidence labels; packet freshness is not graph proof",
        ),
        packet_entry(
            "status_doctor_packet",
            StatusDoctorPacket,
            LevelDiagnosticOnly,
            &["status_rerun", "doctor_rerun", "lifecycle_changed", "sidecar_status_changed"],
            "status/doctor packets report lifecycle and recovery state; they are diagnostic-only",
        ),
    ]
    .into_iter()
    .map(|mut entry| {
        if entry.storage_layer == Sidecar {
            entry.claimability_effect_when_fresh = SidecarOnly;
            entry.claimability_effect_when_stale = NoGraphDbEffect;
        }
        if entry.proof_ladder_level == LevelMutationProof
            || entry.proof_ladder_level == LevelFlowProof
        {
            entry.claimability_effect_when_fresh = Future;
        }
        let stale_effect = entry.claimability_effect_for_state(Stale);
        entry.claimability_effect_when_stale = stale_effect;
        entry
    })
    .collect()
}

pub fn proof_ladder_invalidation_contract() -> Vec<ProofLadderInvalidationRule> {
    use DirtyEvidenceClaimabilityEffect::{
        ConditionalFutureProofOnly, DiagnosticOnly, GraphProofAvailable, GraphProofUnavailable,
        NoGraphDbEffect, NonProofEvidenceOnly, SupportsGraphProofWhenJoined,
    };
    use ProofLadderLevel::{
        CandidateEvidence as LevelCandidateEvidence, DiagnosticOnly as LevelDiagnosticOnly,
        FlowProof as LevelFlowProof, GraphRelationProof as LevelGraphRelationProof,
        MutationProof as LevelMutationProof, SourceNavigationEvidence as LevelSourceNavigation,
        SymbolEvidence as LevelSymbolEvidence, TextEvidence as LevelTextEvidence,
        Unknown as LevelUnknown, Unsupported as LevelUnsupported,
    };

    vec![
        rule(
            LevelTextEvidence,
            "fresh text_evidence proves only source text exists at the cited file/span",
            &["typed graph relation proof", "behavior proof", "hard interrupt eligibility"],
            &["changed file text", "deleted file", "renamed file", "text extraction invalidated"],
            false,
            "warning or diagnostic no-proof fallback; text changes alone are not broken graph behavior",
            false,
            "text_evidence_changed",
            NonProofEvidenceOnly,
            NoGraphDbEffect,
        ),
        rule(
            LevelSymbolEvidence,
            "fresh symbol_evidence proves symbol/entity existence at a source span",
            &["behavior proof", "relation proof without verified graph edge", "runtime semantics"],
            &["changed declaration", "deleted declaration", "renamed symbol", "source span invalidated"],
            true,
            "unknown or diagnostic until symbol/entity evidence is refreshed; not a behavior claim",
            false,
            "symbol_evidence_changed",
            SupportsGraphProofWhenJoined,
            GraphProofUnavailable,
        ),
        rule(
            LevelCandidateEvidence,
            "fresh candidate_evidence can suggest relevant files, symbols, or snippets",
            &["graph proof", "source-code blocker", "hard interrupt eligibility", "benchmark claim"],
            &["changed file content", "candidate source binding changed", "sidecar manifest changed"],
            false,
            "diagnostic-only stale candidate layer; it cannot block claimable graph proof",
            false,
            "candidate_evidence_changed",
            NonProofEvidenceOnly,
            NoGraphDbEffect,
        ),
        rule(
            LevelSourceNavigation,
            "fresh source_navigation_evidence can guide an agent to inspect implementation surfaces",
            &["graph relation proof", "mutation proof", "flow proof", "hard interrupt eligibility"],
            &["changed file content", "source span shifted", "packet handle invalidated"],
            false,
            "diagnostic-only inspection aid; stale navigation does not prove graph breakage",
            false,
            "source_navigation_evidence_changed",
            NonProofEvidenceOnly,
            NoGraphDbEffect,
        ),
        rule(
            LevelGraphRelationProof,
            "fresh graph_relation_proof requires graph/source verification, current lifecycle, source spans where applicable, and provenance where required",
            &["runtime semantics", "typechecker result", "test result", "mutation/flow proof unless separately extracted"],
            &["changed endpoint", "changed edge source span", "deleted edge endpoint", "unsafe lifecycle state"],
            true,
            "graph proof unavailable until reverified/refreshed; stale proof cannot interrupt",
            false,
            "graph_relation_proof_changed",
            GraphProofAvailable,
            GraphProofUnavailable,
        ),
        rule(
            LevelMutationProof,
            "mutation_proof is future/conditional unless current deterministic extractors already emit source-spanned mutation evidence",
            &["MVP4 AST quantization", "runtime mutation behavior", "global interprocedural proof by default"],
            &["changed local mutation site", "changed derived mutation closure", "extractor support changed"],
            true,
            "conditional/future proof unavailable; classify as unknown or unsupported, not blocking proof",
            false,
            "mutation_proof_changed_or_unavailable",
            ConditionalFutureProofOnly,
            GraphProofUnavailable,
        ),
        rule(
            LevelFlowProof,
            "flow_proof is future/conditional unless current deterministic extractors already emit source-spanned flow evidence",
            &["MVP4 micro-flow proof", "runtime dataflow", "complete interprocedural semantics by default"],
            &["changed local flow site", "changed derived flow closure", "extractor support changed"],
            true,
            "conditional/future proof unavailable; classify as unknown or unsupported, not blocking proof",
            false,
            "flow_proof_changed_or_unavailable",
            ConditionalFutureProofOnly,
            GraphProofUnavailable,
        ),
        rule(
            LevelUnknown,
            "unknown means the proof ladder rung cannot be determined",
            &["proof", "hard interrupt eligibility", "source-code blocker"],
            &["insufficient evidence", "unsupported extractor", "bounded output"],
            false,
            "unknown remains unknown and should request inspection or rerun, not block as proof",
            false,
            "proof_ladder_unknown",
            DiagnosticOnly,
            DiagnosticOnly,
        ),
        rule(
            LevelUnsupported,
            "unsupported means CodeGraph does not support the proof claim for this surface",
            &["proof", "hard interrupt eligibility", "source-code blocker"],
            &["unsupported language", "unsupported relation", "unsupported extractor"],
            false,
            "unsupported remains unsupported and non-blocking unless a separate supported graph proof exists",
            false,
            "proof_ladder_unsupported",
            DiagnosticOnly,
            DiagnosticOnly,
        ),
        rule(
            LevelDiagnosticOnly,
            "diagnostic_only describes tool, lifecycle, packet, or sidecar state",
            &["graph proof", "source-code blocker", "hard interrupt eligibility"],
            &["status changed", "diagnostic artifact changed", "sidecar lifecycle changed"],
            false,
            "diagnostic-only output can guide recovery but is not source proof",
            false,
            "diagnostic_only_changed",
            DiagnosticOnly,
            DiagnosticOnly,
        ),
    ]
}

pub fn all_known_dirty_evidence_surfaces() -> Vec<String> {
    dirty_evidence_registry()
        .into_iter()
        .map(|entry| entry.surface_name)
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn entry(
    surface_name: &str,
    evidence_kind: DirtyEvidenceKind,
    proof_ladder_level: ProofLadderLevel,
    graph_proof_possible: bool,
    claimability_effect_when_fresh: DirtyEvidenceClaimabilityEffect,
    claimability_effect_when_stale: DirtyEvidenceClaimabilityEffect,
    source_binding: DirtyEvidenceBinding,
    file_binding: DirtyEvidenceBinding,
    span_binding: DirtyEvidenceBinding,
    entity_binding: DirtyEvidenceBinding,
    edge_binding: DirtyEvidenceBinding,
    sidecar_binding: DirtyEvidenceBinding,
    lifecycle_binding: DirtyEvidenceBinding,
    storage_layer: DirtyEvidenceStorageLayer,
    invalidated_by: &[&str],
    refresh_strategy: DirtyEvidenceRefreshStrategy,
    stale_behavior_in_query: &str,
    stale_behavior_in_context_pack: &str,
    stale_behavior_in_validate_edit: &str,
    stale_behavior_in_mcp: &str,
    status_behavior: &str,
    corrupt_behavior: &str,
    inaccessible_behavior: &str,
    not_applicable_behavior: &str,
) -> DirtyEvidenceRegistryEntry {
    DirtyEvidenceRegistryEntry {
        surface_name: surface_name.to_string(),
        evidence_kind,
        proof_ladder_level,
        graph_proof_possible,
        claimability_effect_when_fresh,
        claimability_effect_when_stale,
        source_binding,
        file_binding,
        span_binding,
        entity_binding,
        edge_binding,
        sidecar_binding,
        lifecycle_binding,
        storage_layer,
        invalidated_by: invalidated_by
            .iter()
            .map(|value| value.to_string())
            .collect(),
        refresh_strategy,
        stale_behavior_in_query: stale_behavior_in_query.to_string(),
        stale_behavior_in_context_pack: stale_behavior_in_context_pack.to_string(),
        stale_behavior_in_validate_edit: stale_behavior_in_validate_edit.to_string(),
        stale_behavior_in_mcp: stale_behavior_in_mcp.to_string(),
        status_behavior: status_behavior.to_string(),
        corrupt_behavior: corrupt_behavior.to_string(),
        inaccessible_behavior: inaccessible_behavior.to_string(),
        not_applicable_behavior: not_applicable_behavior.to_string(),
    }
}

fn sidecar_entry(
    surface_name: &str,
    evidence_kind: DirtyEvidenceKind,
    proof_ladder_level: ProofLadderLevel,
    refresh_strategy: DirtyEvidenceRefreshStrategy,
    invalidated_by: &[&str],
    status_behavior: &str,
) -> DirtyEvidenceRegistryEntry {
    use DirtyEvidenceBinding::{Optional as Opt, Required as Req};
    use DirtyEvidenceClaimabilityEffect::{NoGraphDbEffect, SidecarOnly};
    entry(
        surface_name,
        evidence_kind,
        proof_ladder_level,
        false,
        SidecarOnly,
        NoGraphDbEffect,
        Req,
        Opt,
        Opt,
        Opt,
        Opt,
        Req,
        Req,
        DirtyEvidenceStorageLayer::Sidecar,
        invalidated_by,
        refresh_strategy,
        "stale sidecar evidence is skipped or labeled stale; it is not graph proof",
        "stale sidecar evidence may be omitted or surfaced as non-proof fallback context",
        "stale sidecar evidence cannot block validation or create hard interrupts",
        "stale sidecar evidence is emitted as non-proof sidecar freshness in MCP",
        status_behavior,
        "sidecar corruption is reported as sidecar/index corruption, not graph DB corruption",
        "sidecar inaccessible means the optional sidecar layer is unavailable; graph DB claimability is separate",
        "not applicable when the optional sidecar/layer is disabled or unsupported",
    )
}

fn packet_entry(
    surface_name: &str,
    evidence_kind: DirtyEvidenceKind,
    proof_ladder_level: ProofLadderLevel,
    invalidated_by: &[&str],
    status_behavior: &str,
) -> DirtyEvidenceRegistryEntry {
    use DirtyEvidenceBinding::{NotApplicable as NA, Optional as Opt, Required as Req};
    use DirtyEvidenceClaimabilityEffect::{DiagnosticOnly, NoGraphDbEffect};
    entry(
        surface_name,
        evidence_kind,
        proof_ladder_level,
        false,
        DiagnosticOnly,
        NoGraphDbEffect,
        Opt,
        Opt,
        Opt,
        Opt,
        Opt,
        NA,
        Req,
        DirtyEvidenceStorageLayer::Packet,
        invalidated_by,
        DirtyEvidenceRefreshStrategy::RegeneratePacket,
        "stale packet output is not current evidence and must be regenerated",
        "stale packet output is not reused as current context-pack proof",
        "stale packet output cannot validate the current edit",
        "stale packet output is labeled diagnostic-only in MCP",
        status_behavior,
        "packet corruption is output artifact corruption, not graph DB corruption",
        "packet inaccessible means output details are unavailable until regenerated",
        "not applicable when the packet surface is not requested or unsupported",
    )
}

#[allow(clippy::too_many_arguments)]
fn rule(
    proof_ladder_level: ProofLadderLevel,
    fresh_claim: &str,
    cannot_claim: &[&str],
    changed_file_invalidates: &[&str],
    stale_can_block_claimable_graph_proof: bool,
    stale_severity_effect: &str,
    stale_hard_interrupt_eligible: bool,
    output_change_label: &str,
    fresh_claimability_effect: DirtyEvidenceClaimabilityEffect,
    stale_claimability_effect: DirtyEvidenceClaimabilityEffect,
) -> ProofLadderInvalidationRule {
    ProofLadderInvalidationRule {
        proof_ladder_level,
        fresh_claim: fresh_claim.to_string(),
        cannot_claim: cannot_claim.iter().map(|value| value.to_string()).collect(),
        changed_file_invalidates: changed_file_invalidates
            .iter()
            .map(|value| value.to_string())
            .collect(),
        stale_can_block_claimable_graph_proof,
        stale_severity_effect: stale_severity_effect.to_string(),
        stale_hard_interrupt_eligible,
        output_change_label: output_change_label.to_string(),
        fresh_claimability_effect,
        stale_claimability_effect,
    }
}

fn graph_stale_query() -> &'static str {
    "stale graph evidence is not returned as claimable graph proof"
}

fn graph_stale_context_pack() -> &'static str {
    "stale graph evidence is omitted or labeled no_proof_path_found/unknown"
}

fn graph_stale_validate_edit() -> &'static str {
    "stale graph evidence makes validation proof unavailable until refreshed"
}

fn graph_stale_mcp() -> &'static str {
    "stale graph evidence is emitted as non-claimable lifecycle/evidence state"
}

fn graph_corrupt_behavior() -> &'static str {
    "graph DB/store corruption makes graph proof unavailable and requires graph DB recovery"
}

fn graph_inaccessible_behavior() -> &'static str {
    "graph DB/store inaccessible makes graph proof unavailable and requires recovery"
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::PathBuf};

    use serde_json::{json, Value};

    use super::*;

    fn registry_entry(surface: &str) -> DirtyEvidenceRegistryEntry {
        dirty_evidence_registry()
            .into_iter()
            .find(|entry| entry.surface_name == surface)
            .unwrap_or_else(|| panic!("missing registry entry {surface}"))
    }

    #[test]
    fn dirty_evidence_registry_defined() {
        let registry = dirty_evidence_registry();
        assert!(registry.len() >= 28);
        for entry in registry {
            assert!(!entry.surface_name.is_empty());
            assert!(!entry.invalidated_by.is_empty(), "{}", entry.surface_name);
            assert!(!entry.stale_behavior_in_query.is_empty());
            assert!(!entry.status_behavior.is_empty());
            assert_ne!(
                entry.claimability_effect_when_fresh,
                DirtyEvidenceClaimabilityEffect::NotApplicable
            );
        }
    }

    #[test]
    fn all_known_surfaces_registered_or_not_applicable() {
        let actual = all_known_dirty_evidence_surfaces()
            .into_iter()
            .collect::<BTreeSet<_>>();
        for expected in [
            "graph_entities",
            "graph_edges",
            "source_spans",
            "source_roles",
            "text_evidence",
            "file_manifest",
            "language_source_role_tags",
            "language_capability_tags",
            "parser_fact_bundles",
            "resolver_metadata",
            "project_config_metadata",
            "compiler_lsp_metadata",
            "unresolved_references",
            "PathEvidence",
            "proof_path_caches",
            "graph_delta_snapshots",
            "validation_findings",
            "candidate_spool_packets",
            "candidate_spool_query_index_rows",
            "runtime_vector_chunks",
            "vector_runtime_sidecar_manifest",
            "vector_audit_artifact",
            "binary_candidate_records",
            "nuance_token_records",
            "source_navigation_handles",
            "routing_packet_handles",
            "context_packet_handles",
            "investigation_packet_handles",
            "validate_edit_packet",
            "validation_packet",
            "hard_interrupt_packet",
            "severity_summary",
            "context_pack_packet",
            "status_doctor_packet",
        ] {
            assert!(actual.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn proof_ladder_invalidation_contract_defined() {
        let rules = proof_ladder_invalidation_contract();
        let actual = rules
            .iter()
            .map(|rule| rule.proof_ladder_level)
            .collect::<BTreeSet<_>>();
        for level in ProofLadderLevel::ALL {
            assert!(actual.contains(level), "missing {level:?}");
        }
        assert!(rules.iter().any(|rule| rule.proof_ladder_level
            == ProofLadderLevel::GraphRelationProof
            && rule.stale_can_block_claimable_graph_proof));
    }

    #[test]
    fn claimability_effects_defined() {
        for entry in dirty_evidence_registry() {
            assert_ne!(
                entry.claimability_effect_when_stale,
                DirtyEvidenceClaimabilityEffect::NotApplicable,
                "{}",
                entry.surface_name
            );
            assert!(
                !entry.stale_behavior_in_validate_edit.is_empty(),
                "{}",
                entry.surface_name
            );
            assert!(
                !entry.not_applicable_behavior.is_empty(),
                "{}",
                entry.surface_name
            );
        }
    }

    #[test]
    fn stale_candidate_evidence_not_graph_proof() {
        for surface in [
            "candidate_spool_packets",
            "candidate_spool_query_index_rows",
            "binary_candidate_records",
            "nuance_token_records",
        ] {
            let entry = registry_entry(surface);
            assert_eq!(
                entry.proof_ladder_level,
                ProofLadderLevel::CandidateEvidence
            );
            assert!(!entry.graph_proof_possible);
            assert!(!entry.can_support_graph_proof_in_state(DirtyEvidenceFreshnessState::Stale));
            assert_eq!(
                entry.claimability_effect_for_state(DirtyEvidenceFreshnessState::Stale),
                DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
            );
        }
    }

    #[test]
    fn stale_vector_evidence_not_graph_proof() {
        for surface in [
            "runtime_vector_chunks",
            "vector_runtime_sidecar_manifest",
            "vector_audit_artifact",
        ] {
            let entry = registry_entry(surface);
            assert!(!entry.graph_proof_possible);
            assert!(!entry.can_support_graph_proof_in_state(DirtyEvidenceFreshnessState::Stale));
            assert_eq!(
                entry.claimability_effect_for_state(DirtyEvidenceFreshnessState::Corrupt),
                DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
            );
        }
    }

    #[test]
    fn stale_source_navigation_not_graph_proof() {
        for surface in [
            "source_navigation_handles",
            "routing_packet_handles",
            "context_packet_handles",
            "investigation_packet_handles",
        ] {
            let entry = registry_entry(surface);
            assert_eq!(
                entry.proof_ladder_level,
                ProofLadderLevel::SourceNavigationEvidence
            );
            assert!(!entry.graph_proof_possible);
            assert!(
                !entry.can_claim_graph_relation_proof_in_state(DirtyEvidenceFreshnessState::Stale)
            );
        }
    }

    #[test]
    fn text_evidence_not_broken_graph_behavior() {
        let entry = registry_entry("text_evidence");
        assert_eq!(entry.proof_ladder_level, ProofLadderLevel::TextEvidence);
        assert!(!entry.graph_proof_possible);
        assert!(entry
            .stale_behavior_in_validate_edit
            .contains("not graph breakage"));
        assert_eq!(
            entry.claimability_effect_for_state(DirtyEvidenceFreshnessState::Stale),
            DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
        );
    }

    #[test]
    fn language_capability_surfaces_gate_proof_without_creating_it() {
        for surface in [
            "language_capability_tags",
            "project_config_metadata",
            "unresolved_references",
        ] {
            let entry = registry_entry(surface);
            assert!(
                !entry.can_support_graph_proof_in_state(DirtyEvidenceFreshnessState::Fresh),
                "{surface} should gate or diagnose proof rather than create graph proof"
            );
            assert!(
                !entry.can_claim_graph_relation_proof_in_state(DirtyEvidenceFreshnessState::Stale)
            );
        }

        for surface in [
            "parser_fact_bundles",
            "resolver_metadata",
            "compiler_lsp_metadata",
        ] {
            let entry = registry_entry(surface);
            assert!(entry.graph_proof_possible, "{surface}");
            assert!(
                !entry.can_claim_graph_relation_proof_in_state(DirtyEvidenceFreshnessState::Stale)
            );
            assert_eq!(
                entry.claimability_effect_for_state(DirtyEvidenceFreshnessState::Stale),
                DirtyEvidenceClaimabilityEffect::GraphProofUnavailable
            );
        }

        let unresolved = registry_entry("unresolved_references");
        assert_eq!(
            unresolved.claimability_effect_for_state(DirtyEvidenceFreshnessState::Stale),
            DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
        );
        assert!(unresolved
            .stale_behavior_in_validate_edit
            .contains("cannot hard-interrupt"));
    }

    #[test]
    fn graph_db_corrupt_distinct_from_sidecar_corrupt() {
        let graph = registry_entry("graph_edges");
        let sidecar = registry_entry("candidate_spool_query_index_rows");
        assert_eq!(graph.storage_layer, DirtyEvidenceStorageLayer::GraphDb);
        assert_eq!(sidecar.storage_layer, DirtyEvidenceStorageLayer::Sidecar);
        assert_ne!(graph.corrupt_behavior, sidecar.corrupt_behavior);
        assert!(graph.state_blocks_graph_db_claimability(DirtyEvidenceFreshnessState::Corrupt));
        assert!(!sidecar.state_blocks_graph_db_claimability(DirtyEvidenceFreshnessState::Corrupt));
    }

    #[test]
    fn graph_claimable_sidecar_corrupt_possible() {
        let graph = registry_entry("graph_edges");
        let sidecar = registry_entry("candidate_spool_query_index_rows");
        assert!(graph.can_claim_graph_relation_proof_in_state(DirtyEvidenceFreshnessState::Fresh));
        assert!(!sidecar.state_blocks_graph_db_claimability(DirtyEvidenceFreshnessState::Corrupt));
        assert_eq!(
            sidecar.claimability_effect_for_state(DirtyEvidenceFreshnessState::Corrupt),
            DirtyEvidenceClaimabilityEffect::NoGraphDbEffect
        );
    }

    #[test]
    fn mutation_flow_proof_future_or_conditional() {
        for level in [ProofLadderLevel::MutationProof, ProofLadderLevel::FlowProof] {
            let rule = proof_ladder_invalidation_contract()
                .into_iter()
                .find(|rule| rule.proof_ladder_level == level)
                .unwrap_or_else(|| panic!("missing {level:?}"));
            assert_eq!(
                rule.fresh_claimability_effect,
                DirtyEvidenceClaimabilityEffect::ConditionalFutureProofOnly
            );
            assert!(rule.fresh_claim.contains("future/conditional"));
            assert!(rule.cannot_claim.iter().any(|claim| claim.contains("MVP4")));
        }
    }

    #[test]
    fn schema_compatibility_preserved() {
        let registry_json =
            serde_json::to_value(dirty_evidence_registry()).expect("registry serializes");
        assert!(registry_json
            .as_array()
            .is_some_and(|items| !items.is_empty()));
        let contract_json = serde_json::to_value(proof_ladder_invalidation_contract())
            .expect("contract serializes");
        assert!(contract_json
            .as_array()
            .is_some_and(|items| !items.is_empty()));

        let workspace = workspace_root();
        for schema in [
            "docs/schemas/agent-json/common.schema.json",
            "docs/schemas/agent-json/validation_packet_agent_json.schema.json",
            "docs/schemas/agent-json/hard_interrupt_agent_json.schema.json",
            "docs/schemas/agent-json/context_pack_agent_json.schema.json",
            "docs/schemas/agent-json/status_compact_json.schema.json",
            "docs/schemas/agent-json/doctor_compact_json.schema.json",
        ] {
            let path = workspace.join(schema);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {path:?}: {error}"));
            let value: Value = serde_json::from_str(&text)
                .unwrap_or_else(|error| panic!("parse {path:?}: {error}"));
            assert!(value.is_object(), "{schema}");
        }
        let common: Value = serde_json::from_str(
            &std::fs::read_to_string(workspace.join("docs/schemas/agent-json/common.schema.json"))
                .expect("read common schema"),
        )
        .expect("parse common schema");
        assert!(common.pointer("/$defs/dirty_evidence_summary").is_some());
        assert!(common
            .pointer("/$defs/proof_ladder_change_counts")
            .is_some());
        assert!(common.pointer("/$defs/sidecar_statuses").is_some());
    }

    #[test]
    fn no_dot_codegraph_mutation() {
        assert!(
            !workspace_root().join(".codegraph").exists(),
            "normal repo-local .codegraph must not be created by contract tests"
        );
    }

    #[test]
    fn artifact_dump_when_env_requested() {
        let Some(output_dir) = std::env::var_os("CODEGRAPH_DIRTY_EVIDENCE_ARTIFACT_DIR") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        std::fs::create_dir_all(&output_dir)
            .unwrap_or_else(|error| panic!("create artifact dir {output_dir:?}: {error}"));

        let registry = json!({
            "schema_version": DIRTY_EVIDENCE_REGISTRY_SCHEMA_VERSION,
            "status": "complete",
            "source": "crates/codegraph-core/src/dirty_evidence.rs",
            "surface_count": dirty_evidence_registry().len(),
            "surfaces": dirty_evidence_registry()
        });
        let registry_path = output_dir.join("dirty_evidence_registry.json");
        std::fs::write(
            &registry_path,
            serde_json::to_string_pretty(&registry).expect("registry artifact serializes"),
        )
        .unwrap_or_else(|error| panic!("write {registry_path:?}: {error}"));

        let contract = json!({
            "schema_version": PROOF_LADDER_INVALIDATION_CONTRACT_SCHEMA_VERSION,
            "status": "complete",
            "source": "crates/codegraph-core/src/dirty_evidence.rs",
            "rule_count": proof_ladder_invalidation_contract().len(),
            "rules": proof_ladder_invalidation_contract()
        });
        let contract_path = output_dir.join("proof_ladder_invalidation_contract.json");
        std::fs::write(
            &contract_path,
            serde_json::to_string_pretty(&contract).expect("proof ladder artifact serializes"),
        )
        .unwrap_or_else(|error| panic!("write {contract_path:?}: {error}"));
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root")
            .to_path_buf()
    }
}
