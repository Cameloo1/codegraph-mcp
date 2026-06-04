use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{EntityKind, EvidenceRole, Exactness, RelationKind, SourceSpan};

pub const VALIDATION_PACKET_SCHEMA_VERSION: u32 = 1;
pub const HARD_INTERRUPT_PACKET_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationPacketKind {
    GraphValidationPacket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardInterruptPacketKind {
    HardInterrupt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationPacketStatus {
    BlockingGraphError,
    Warning,
    Ok,
    DiagnosticOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationClassification {
    Block,
    Warn,
    Unknown,
    Unsupported,
    Degraded,
    Diagnostic,
}

impl ValidationClassification {
    pub const fn blocking_level(self) -> ValidationBlockingLevel {
        match self {
            Self::Block => ValidationBlockingLevel::Blocking,
            Self::Warn | Self::Degraded => ValidationBlockingLevel::Warning,
            Self::Unknown | Self::Unsupported => ValidationBlockingLevel::Unknown,
            Self::Diagnostic => ValidationBlockingLevel::DiagnosticOnly,
        }
    }

    pub const fn is_blocking(self) -> bool {
        matches!(self, Self::Block)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationBlockingLevel {
    Blocking,
    Warning,
    Unknown,
    DiagnosticOnly,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationRuleKind {
    DanglingTarget,
    BrokenContract,
    ProofIntegrity,
    SourceRoleBoundary,
    LifecycleIntegrity,
    UnsupportedRelationBoundary,
    ClosureBudgetBoundary,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportedRelationStatus {
    ExactBlockingCandidate,
    ExactWarningCandidate,
    ActivationGated,
    Unsupported,
    DiagnosticOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationProofRequirement {
    ReverifiedGraphSourceProof,
    ReverifiedGraphIntegrity,
    LifecycleCurrentClaimable,
    DiagnosticOnly,
    NotGraphProof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSourceSpanRequirement {
    RequiredForClaimableGraphFact,
    Optional,
    NotApplicable,
}

impl ValidationSourceSpanRequirement {
    pub const fn requires_span(self) -> bool {
        matches!(self, Self::RequiredForClaimableGraphFact)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationProvenanceRequirement {
    RequiredForDerivedEdges,
    Optional,
    NotApplicable,
}

impl ValidationProvenanceRequirement {
    pub const fn requires_provenance(self) -> bool {
        matches!(self, Self::RequiredForDerivedEdges)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSourceRoleRequirement {
    ProductionOnlyByDefault,
    RolePreserved,
    TestImpactAllowed,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationLifecycleRequirement {
    ClaimableCurrentDb,
    DiagnosticReadOnly,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationProofStatus {
    ReverifiedGraphSourceProof,
    ReverifiedGraphIntegrity,
    NotGraphProof,
    StaleOrNonClaimableDb,
    UnsupportedRelation,
    OverBudgetDegraded,
    NeedsReverification,
    MissingRequiredSourceSpan,
    MissingRequiredProvenance,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationEvidenceKind {
    GraphSource,
    GraphIntegrity,
    TextEvidence,
    Candidate,
    Vector,
    SourceNavigation,
    PathEvidence,
    Routing,
    Nuance,
    Binary,
    Lifecycle,
    Diagnostic,
}

impl ValidationEvidenceKind {
    pub const fn can_support_blocking_graph_proof(self) -> bool {
        matches!(self, Self::GraphSource | Self::GraphIntegrity)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationRule {
    pub validation_rule_id: String,
    pub rule_kind: ValidationRuleKind,
    pub relation_kind: Option<RelationKind>,
    pub invariant: String,
    pub activation_condition: String,
    pub proof_requirement: ValidationProofRequirement,
    pub source_span_requirement: ValidationSourceSpanRequirement,
    pub provenance_requirement: ValidationProvenanceRequirement,
    pub source_role_requirement: ValidationSourceRoleRequirement,
    pub lifecycle_requirement: ValidationLifecycleRequirement,
    pub supported_relation_status: SupportedRelationStatus,
    pub default_classification_when_unsupported: ValidationClassification,
    pub docs_summary: String,
}

impl ValidationRule {
    pub fn exact_blocking(
        validation_rule_id: impl Into<String>,
        rule_kind: ValidationRuleKind,
        relation_kind: Option<RelationKind>,
        invariant: impl Into<String>,
        docs_summary: impl Into<String>,
    ) -> Self {
        Self {
            validation_rule_id: validation_rule_id.into(),
            rule_kind,
            relation_kind,
            invariant: invariant.into(),
            activation_condition: "exact source-spanned relation plus graph/source re-verification"
                .to_string(),
            proof_requirement: ValidationProofRequirement::ReverifiedGraphSourceProof,
            source_span_requirement: ValidationSourceSpanRequirement::RequiredForClaimableGraphFact,
            provenance_requirement: ValidationProvenanceRequirement::Optional,
            source_role_requirement: ValidationSourceRoleRequirement::ProductionOnlyByDefault,
            lifecycle_requirement: ValidationLifecycleRequirement::ClaimableCurrentDb,
            supported_relation_status: SupportedRelationStatus::ExactBlockingCandidate,
            default_classification_when_unsupported: ValidationClassification::Unknown,
            docs_summary: docs_summary.into(),
        }
    }

    pub fn diagnostic(
        validation_rule_id: impl Into<String>,
        invariant: impl Into<String>,
        docs_summary: impl Into<String>,
    ) -> Self {
        Self {
            validation_rule_id: validation_rule_id.into(),
            rule_kind: ValidationRuleKind::Diagnostic,
            relation_kind: None,
            invariant: invariant.into(),
            activation_condition: "diagnostic observation only".to_string(),
            proof_requirement: ValidationProofRequirement::DiagnosticOnly,
            source_span_requirement: ValidationSourceSpanRequirement::NotApplicable,
            provenance_requirement: ValidationProvenanceRequirement::NotApplicable,
            source_role_requirement: ValidationSourceRoleRequirement::NotApplicable,
            lifecycle_requirement: ValidationLifecycleRequirement::DiagnosticReadOnly,
            supported_relation_status: SupportedRelationStatus::DiagnosticOnly,
            default_classification_when_unsupported: ValidationClassification::Diagnostic,
            docs_summary: docs_summary.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationLifecycleState {
    pub claimable: bool,
    pub current: bool,
    pub stale: bool,
    pub foreign: bool,
    pub schema_mismatched: bool,
    pub dirty: bool,
    pub partial: bool,
    pub non_claimable_reason: Option<String>,
}

impl ValidationLifecycleState {
    pub fn claimable_current() -> Self {
        Self {
            claimable: true,
            current: true,
            stale: false,
            foreign: false,
            schema_mismatched: false,
            dirty: false,
            partial: false,
            non_claimable_reason: None,
        }
    }

    pub fn stale_non_claimable(reason: impl Into<String>) -> Self {
        Self {
            claimable: false,
            current: false,
            stale: true,
            foreign: false,
            schema_mismatched: false,
            dirty: false,
            partial: false,
            non_claimable_reason: Some(reason.into()),
        }
    }

    pub const fn is_claimable_current(&self) -> bool {
        self.claimable
            && self.current
            && !self.stale
            && !self.foreign
            && !self.schema_mismatched
            && !self.dirty
            && !self.partial
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationEvidenceItem {
    pub evidence_kind: ValidationEvidenceKind,
    pub evidence_id: Option<String>,
    pub proof_status: ValidationProofStatus,
    pub graph_proof: bool,
    pub claimable: bool,
    pub reason: String,
}

impl ValidationEvidenceItem {
    pub fn graph_source(evidence_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            evidence_kind: ValidationEvidenceKind::GraphSource,
            evidence_id: Some(evidence_id.into()),
            proof_status: ValidationProofStatus::ReverifiedGraphSourceProof,
            graph_proof: true,
            claimable: true,
            reason: reason.into(),
        }
    }

    pub fn graph_integrity(evidence_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            evidence_kind: ValidationEvidenceKind::GraphIntegrity,
            evidence_id: Some(evidence_id.into()),
            proof_status: ValidationProofStatus::ReverifiedGraphIntegrity,
            graph_proof: true,
            claimable: true,
            reason: reason.into(),
        }
    }

    pub fn non_graph(
        evidence_kind: ValidationEvidenceKind,
        evidence_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            evidence_kind,
            evidence_id: Some(evidence_id.into()),
            proof_status: ValidationProofStatus::NotGraphProof,
            graph_proof: false,
            claimable: false,
            reason: reason.into(),
        }
    }

    pub const fn can_support_blocking_graph_proof(&self) -> bool {
        self.graph_proof && self.claimable && self.evidence_kind.can_support_blocking_graph_proof()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReverificationInput {
    pub lifecycle: ValidationLifecycleState,
    pub relation_supported: bool,
    pub relation_exact: bool,
    pub graph_source_relation_reverified: bool,
    pub integrity_condition_reverified: bool,
    pub integrity_issue_present: bool,
    pub source_span_required: bool,
    pub source_span_present: bool,
    pub provenance_required: bool,
    pub provenance_present: bool,
    pub source_role_allowed: bool,
    pub over_budget: bool,
    pub unsupported_relation: bool,
    pub evidence_items: Vec<ValidationEvidenceItem>,
    pub reason: String,
}

impl ValidationReverificationInput {
    pub fn exact_graph_source(
        lifecycle: ValidationLifecycleState,
        evidence_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        let reason = reason.into();
        Self {
            lifecycle,
            relation_supported: true,
            relation_exact: true,
            graph_source_relation_reverified: true,
            integrity_condition_reverified: false,
            integrity_issue_present: false,
            source_span_required: true,
            source_span_present: true,
            provenance_required: false,
            provenance_present: true,
            source_role_allowed: true,
            over_budget: false,
            unsupported_relation: false,
            evidence_items: vec![ValidationEvidenceItem::graph_source(evidence_id, &reason)],
            reason,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReverification {
    pub db_claimable_current: bool,
    pub relation_supported: bool,
    pub relation_exact: bool,
    pub graph_source_relation_reverified: bool,
    pub integrity_condition_reverified: bool,
    pub source_span_required: bool,
    pub source_span_present: bool,
    pub provenance_required: bool,
    pub provenance_present: bool,
    pub source_role_allowed: bool,
    pub evidence_can_support_blocking: bool,
    pub over_budget: bool,
    pub unsupported_relation: bool,
    pub blocking_graph_source_relation_allowed: bool,
    pub blocking_integrity_condition_allowed: bool,
    pub downgrade_reason: Option<String>,
}

pub fn reverify_validation_graph_source_contract(
    input: &ValidationReverificationInput,
) -> ValidationReverification {
    let db_claimable_current = input.lifecycle.is_claimable_current();
    let evidence_can_support_blocking = input
        .evidence_items
        .iter()
        .any(ValidationEvidenceItem::can_support_blocking_graph_proof);
    let relation_supported = input.relation_supported && !input.unsupported_relation;
    let source_span_ok = !input.source_span_required || input.source_span_present;
    let provenance_ok = !input.provenance_required || input.provenance_present;
    let integrity_issue_present = input.integrity_issue_present
        || (input.source_span_required && !input.source_span_present)
        || (input.provenance_required && !input.provenance_present);

    let common_blocking_preconditions = db_claimable_current
        && relation_supported
        && input.relation_exact
        && input.source_role_allowed
        && evidence_can_support_blocking
        && !input.over_budget;

    let blocking_graph_source_relation_allowed = common_blocking_preconditions
        && input.graph_source_relation_reverified
        && source_span_ok
        && provenance_ok;
    let blocking_integrity_condition_allowed = common_blocking_preconditions
        && input.integrity_condition_reverified
        && integrity_issue_present;

    let downgrade_reason = if input.over_budget {
        Some("closure_or_packet_budget_hit".to_string())
    } else if input.unsupported_relation || !input.relation_supported {
        Some("relation_unsupported_or_not_enabled".to_string())
    } else if !db_claimable_current {
        input
            .lifecycle
            .non_claimable_reason
            .clone()
            .or_else(|| Some("db_lifecycle_not_claimable_current".to_string()))
    } else if !evidence_can_support_blocking {
        Some("evidence_is_not_reverified_graph_source_proof".to_string())
    } else if !input.relation_exact {
        Some("relation_not_exact".to_string())
    } else if !input.source_role_allowed {
        Some("source_role_not_allowed_for_production_proof".to_string())
    } else if input.graph_source_relation_reverified && !source_span_ok {
        Some("required_source_span_missing".to_string())
    } else if input.graph_source_relation_reverified && !provenance_ok {
        Some("required_provenance_missing".to_string())
    } else if input.integrity_condition_reverified && !integrity_issue_present {
        Some("integrity_condition_not_failed".to_string())
    } else if !input.graph_source_relation_reverified && !input.integrity_condition_reverified {
        Some("graph_source_reverification_not_performed".to_string())
    } else {
        None
    };

    ValidationReverification {
        db_claimable_current,
        relation_supported,
        relation_exact: input.relation_exact,
        graph_source_relation_reverified: input.graph_source_relation_reverified,
        integrity_condition_reverified: input.integrity_condition_reverified,
        source_span_required: input.source_span_required,
        source_span_present: input.source_span_present,
        provenance_required: input.provenance_required,
        provenance_present: input.provenance_present,
        source_role_allowed: input.source_role_allowed,
        evidence_can_support_blocking,
        over_budget: input.over_budget,
        unsupported_relation: input.unsupported_relation,
        blocking_graph_source_relation_allowed,
        blocking_integrity_condition_allowed,
        downgrade_reason,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub finding_id: String,
    pub validation_rule_id: String,
    pub invariant: String,
    pub classification: ValidationClassification,
    pub blocking_level: ValidationBlockingLevel,
    pub integrity_kind: Option<String>,
    pub proof_level: String,
    pub proof_strength: String,
    pub proof_status: ValidationProofStatus,
    pub affected_evidence: Value,
    pub affected_delta: Value,
    pub affected_edge: Value,
    pub affected_entity: Value,
    pub file: Option<String>,
    pub affected_file: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub source_role: Option<EvidenceRole>,
    pub relation_kind: Option<RelationKind>,
    pub exactness: Option<Exactness>,
    pub provenance: Value,
    pub old_fact_claim_state: String,
    pub new_fact_claim_state: String,
    pub lifecycle: ValidationLifecycleState,
    pub reverified_graph_source_proof: bool,
    pub evidence_items: Vec<ValidationEvidenceItem>,
    pub reason: String,
    pub recommended_fix: Option<String>,
    pub suggested_next_steps: Vec<String>,
    pub unknowns: Vec<String>,
    pub diagnostics: Vec<String>,
    pub expansion_handle: Option<String>,
}

impl ValidationFinding {
    fn sync_agent_json_aliases(&mut self) {
        if self.file.is_none() {
            self.file = self.affected_file.clone().or_else(|| {
                self.source_span
                    .as_ref()
                    .map(|span| crate::normalize_repo_relative_path(&span.repo_relative_path))
            });
        }
        if self.affected_file.is_none() {
            self.affected_file = self.file.clone();
        }
        if self.affected_evidence.is_null() {
            self.affected_evidence = json!(&self.evidence_items);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptEligibility {
    pub eligible: bool,
    pub reason: String,
    pub disqualifiers: Vec<String>,
    pub lifecycle_ok: bool,
    pub claimability_ok: bool,
    pub classification_ok: bool,
    pub reverified_graph_source_proof: bool,
    pub source_span_ok: bool,
    pub source_role_ok: bool,
    pub provenance_ok: bool,
    pub evidence_kind_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptFixHint {
    pub recommended_fix: String,
    pub suggested_next_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptExpansionHandle {
    pub handle: String,
    pub target: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterruptSourceFinding {
    pub finding_id: String,
    pub validation_rule_id: String,
    pub classification: ValidationClassification,
    pub blocking_level: ValidationBlockingLevel,
    pub file: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub relation_kind: Option<RelationKind>,
    pub integrity_kind: Option<String>,
    pub reason: String,
    pub recommended_fix: Option<String>,
    pub suggested_next_steps: Vec<String>,
    pub expansion_handle: Option<String>,
}

impl InterruptSourceFinding {
    pub fn from_validation_finding(finding: &ValidationFinding) -> Self {
        Self {
            finding_id: finding.finding_id.clone(),
            validation_rule_id: finding.validation_rule_id.clone(),
            classification: finding.classification,
            blocking_level: finding.blocking_level,
            file: finding
                .file
                .clone()
                .or_else(|| finding.affected_file.clone()),
            source_span: finding.source_span.clone(),
            relation_kind: finding.relation_kind,
            integrity_kind: finding.integrity_kind.clone(),
            reason: finding.reason.clone(),
            recommended_fix: finding.recommended_fix.clone(),
            suggested_next_steps: finding.suggested_next_steps.clone(),
            expansion_handle: finding.expansion_handle.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardInterruptError {
    pub error_id: String,
    pub validation_rule_id: String,
    pub source_finding_id: String,
    pub classification: ValidationClassification,
    pub blocking_level: ValidationBlockingLevel,
    pub rule_kind: ValidationRuleKind,
    pub relation_kind: Option<RelationKind>,
    pub integrity_kind: Option<String>,
    pub invariant: String,
    pub severity: String,
    pub file: String,
    pub source_span: SourceSpan,
    pub symbol: Option<String>,
    pub source_symbol_summary: Value,
    pub target: Option<String>,
    pub missing_target_summary: Value,
    pub message: String,
    pub template_kind: String,
    pub exact_proof_reason: String,
    pub reverified_graph_source_proof: bool,
    pub source_role: Option<EvidenceRole>,
    pub exactness: Option<Exactness>,
    pub provenance: Value,
    pub claimability: Value,
    pub lifecycle: ValidationLifecycleState,
    pub proof_strength: String,
    pub proof_status: ValidationProofStatus,
    pub recommended_fix: String,
    pub suggested_next_steps: Vec<String>,
    pub evidence_items: Vec<ValidationEvidenceItem>,
    pub expansion_handle: InterruptExpansionHandle,
    pub eligibility: InterruptEligibility,
    pub fix_hint: InterruptFixHint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterruptPacketSummary {
    pub error_count: usize,
    pub warning_count: usize,
    pub unknown_count: usize,
    pub diagnostic_count: usize,
    pub top_error_validation_rule_id: Option<String>,
    pub top_error_file: Option<String>,
    pub top_error_source_span: Option<SourceSpan>,
    pub top_error_recommended_fix: Option<String>,
    pub top_error_suggested_next_steps: Vec<String>,
    pub critical_safety_fields_preserved: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardInterruptPacket {
    pub schema_version: u32,
    pub packet_kind: HardInterruptPacketKind,
    pub status: ValidationPacketStatus,
    pub must_fix_before_continuing: bool,
    pub hard_interrupt_available: bool,
    pub changed_files: Vec<String>,
    pub source_validation_packet_ref: Option<String>,
    pub source_validation_packet_summary: Option<Value>,
    pub source_validation_packet_kind: ValidationPacketKind,
    pub errors: Vec<HardInterruptError>,
    pub warnings: Vec<InterruptSourceFinding>,
    pub unknowns: Vec<InterruptSourceFinding>,
    pub diagnostics: Vec<InterruptSourceFinding>,
    pub error_count: usize,
    pub warnings_summary: Value,
    pub unknowns_summary: Value,
    pub diagnostics_summary: Value,
    pub summary: InterruptPacketSummary,
    pub claimability: Value,
    pub lifecycle: Value,
    pub proof_ladder_changes: Value,
    pub stale_unsafe_blockers: Vec<String>,
    pub validation_rules_evaluated: Vec<ValidationRule>,
    pub validation_rules_skipped: Vec<ValidationRule>,
    pub aggregate_counts: Value,
    pub omitted_count: usize,
    pub expansion_handles: Vec<InterruptExpansionHandle>,
    pub generated_at: String,
}

impl HardInterruptPacket {
    pub const fn critical_safety_field_names() -> &'static [&'static str] {
        &[
            "packet_kind",
            "status",
            "must_fix_before_continuing",
            "hard_interrupt_available",
            "errors",
            "error_count",
            "warnings_summary",
            "unknowns_summary",
            "diagnostics_summary",
            "summary",
            "claimability",
            "lifecycle",
            "stale_unsafe_blockers",
            "proof_ladder_changes",
            "validation_rules_evaluated",
            "validation_rules_skipped",
            "omitted_count",
            "expansion_handles",
        ]
    }

    pub fn compact_agent_json(&self, max_items_per_bucket: usize) -> Value {
        let error_limit = max_items_per_bucket.max(1);
        let detail_limit = max_items_per_bucket;
        let mut sorted_errors = self.errors.iter().collect::<Vec<_>>();
        sorted_errors.sort_by(|left, right| hard_interrupt_error_order(left, right));
        let compact_errors = sorted_errors
            .iter()
            .take(error_limit)
            .map(|error| hard_interrupt_compact_error_json(error))
            .collect::<Vec<_>>();
        let compact_warnings = self
            .warnings
            .iter()
            .take(detail_limit)
            .map(hard_interrupt_compact_source_finding_json)
            .collect::<Vec<_>>();
        let compact_unknowns = self
            .unknowns
            .iter()
            .take(detail_limit)
            .map(hard_interrupt_compact_source_finding_json)
            .collect::<Vec<_>>();
        let compact_diagnostics = self
            .diagnostics
            .iter()
            .take(detail_limit)
            .map(hard_interrupt_compact_source_finding_json)
            .collect::<Vec<_>>();
        let omitted = self.omitted_count
            + self.errors.len().saturating_sub(error_limit)
            + self.warnings.len().saturating_sub(detail_limit)
            + self.unknowns.len().saturating_sub(detail_limit)
            + self.diagnostics.len().saturating_sub(detail_limit);
        let expansion_handles =
            hard_interrupt_compact_expansion_handles(&sorted_errors, error_limit, omitted);

        let mut value = json!({
            "schema_version": self.schema_version,
            "packet_kind": self.packet_kind,
            "status": self.status,
            "must_fix_before_continuing": self.must_fix_before_continuing,
            "hard_interrupt_available": self.hard_interrupt_available,
            "changed_files": &self.changed_files,
            "source_validation_packet_ref": &self.source_validation_packet_ref,
            "source_validation_packet_summary": hard_interrupt_compact_source_packet_summary(
                self.source_validation_packet_summary.as_ref()
            ),
            "source_validation_packet_kind": self.source_validation_packet_kind,
            "errors": compact_errors,
            "warnings": compact_warnings,
            "unknowns": compact_unknowns,
            "diagnostics": compact_diagnostics,
            "error_count": self.errors.len(),
            "warnings_summary": hard_interrupt_source_findings_summary(&self.warnings, detail_limit),
            "unknowns_summary": hard_interrupt_source_findings_summary(&self.unknowns, detail_limit),
            "diagnostics_summary": hard_interrupt_source_findings_summary(&self.diagnostics, detail_limit),
            "summary": &self.summary,
            "claimability": hard_interrupt_compact_claimability_json(&self.claimability),
            "lifecycle": hard_interrupt_compact_lifecycle_json(&self.lifecycle),
            "proof_ladder_changes": hard_interrupt_compact_proof_ladder_changes_json(&self.proof_ladder_changes),
            "stale_unsafe_blockers": &self.stale_unsafe_blockers,
            "validation_rules_evaluated": hard_interrupt_compact_rules_json(&self.validation_rules_evaluated),
            "validation_rules_skipped": hard_interrupt_compact_rules_json(&self.validation_rules_skipped),
            "aggregate_counts": hard_interrupt_error_aggregate_counts(&self.errors),
            "omitted_count": omitted,
            "expansion_handles": expansion_handles,
            "generated_at": &self.generated_at,
        });
        let critical_safety_fields_preserved = Self::critical_safety_fields_preserved_in(&value);
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "critical_safety_fields_preserved".to_string(),
                json!(critical_safety_fields_preserved),
            );
        }
        value
    }

    pub fn critical_safety_fields_preserved_in(value: &Value) -> bool {
        let summary = &value["summary"];
        let top_error = value
            .get("errors")
            .and_then(Value::as_array)
            .and_then(|items| items.first());
        Self::critical_safety_field_names()
            .iter()
            .all(|field| value.get(*field).is_some())
            && value.get("error_count").and_then(Value::as_u64).is_some()
            && summary.get("error_count").is_some()
            && summary.get("top_error_validation_rule_id").is_some()
            && summary.get("top_error_file").is_some()
            && summary.get("top_error_source_span").is_some()
            && summary.get("top_error_recommended_fix").is_some()
            && summary.get("top_error_suggested_next_steps").is_some()
            && top_error
                .map(|error| {
                    [
                        "validation_rule_id",
                        "classification",
                        "blocking_level",
                        "file",
                        "source_span",
                        "message",
                        "exact_proof_reason",
                        "recommended_fix",
                        "suggested_next_steps",
                        "reverified_graph_source_proof",
                        "claimability",
                        "lifecycle",
                        "expansion_handle",
                    ]
                    .iter()
                    .all(|field| error.get(*field).is_some())
                })
                .unwrap_or(false)
    }

    pub fn from_validation_packet(
        packet: &ValidationPacket,
        generated_at: impl Into<String>,
    ) -> Option<Self> {
        let rule_by_id = packet
            .validation_rules_evaluated
            .iter()
            .chain(packet.validation_rules_skipped.iter())
            .map(|rule| (rule.validation_rule_id.as_str(), rule))
            .collect::<BTreeMap<_, _>>();
        let mut errors = Vec::new();
        let mut expansion_handles = Vec::new();

        for finding in &packet.blocking_errors {
            let rule = rule_by_id.get(finding.validation_rule_id.as_str()).copied();
            let eligibility =
                interrupt_eligibility_for_finding(finding, rule, &packet.claimability);
            if !eligibility.eligible {
                continue;
            }
            let Some(rule) = rule else {
                continue;
            };
            let Some(error) =
                HardInterruptError::from_validation_finding(finding, rule, eligibility)
            else {
                continue;
            };
            expansion_handles.push(error.expansion_handle.clone());
            errors.push(error);
        }

        if errors.is_empty() {
            return None;
        }

        hard_interrupt_sort_errors(&mut errors);
        expansion_handles = errors
            .iter()
            .map(|error| error.expansion_handle.clone())
            .collect();
        let top_error = errors.first();
        let warnings = packet
            .warnings
            .iter()
            .map(InterruptSourceFinding::from_validation_finding)
            .collect::<Vec<_>>();
        let unknowns = packet
            .unknowns
            .iter()
            .map(InterruptSourceFinding::from_validation_finding)
            .collect::<Vec<_>>();
        let diagnostics = packet
            .diagnostics
            .iter()
            .map(InterruptSourceFinding::from_validation_finding)
            .collect::<Vec<_>>();
        Some(Self {
            schema_version: HARD_INTERRUPT_PACKET_SCHEMA_VERSION,
            packet_kind: HardInterruptPacketKind::HardInterrupt,
            status: ValidationPacketStatus::BlockingGraphError,
            must_fix_before_continuing: true,
            hard_interrupt_available: true,
            changed_files: packet.changed_files.clone(),
            source_validation_packet_ref: None,
            source_validation_packet_summary: Some(json!({
                "schema_version": packet.schema_version,
                "packet_kind": packet.packet_kind,
                "status": packet.status,
                "must_fix_before_continuing": packet.must_fix_before_continuing,
                "hard_interrupt_available": true,
                "blocking_error_count": packet.blocking_errors.len(),
                "warning_count": packet.warnings.len(),
                "unknown_count": packet.unknowns.len(),
                "diagnostic_count": packet.diagnostics.len(),
            })),
            source_validation_packet_kind: packet.packet_kind,
            warnings_summary: hard_interrupt_source_findings_summary(&warnings, 3),
            unknowns_summary: hard_interrupt_source_findings_summary(&unknowns, 3),
            diagnostics_summary: hard_interrupt_source_findings_summary(&diagnostics, 3),
            summary: InterruptPacketSummary {
                error_count: errors.len(),
                warning_count: packet.warnings.len(),
                unknown_count: packet.unknowns.len(),
                diagnostic_count: packet.diagnostics.len(),
                top_error_validation_rule_id: top_error
                    .map(|error| error.validation_rule_id.clone()),
                top_error_file: top_error.map(|error| error.file.clone()),
                top_error_source_span: top_error.map(|error| error.source_span.clone()),
                top_error_recommended_fix: top_error.map(|error| error.recommended_fix.clone()),
                top_error_suggested_next_steps: top_error
                    .map(|error| error.suggested_next_steps.clone())
                    .unwrap_or_default(),
                critical_safety_fields_preserved: true,
            },
            claimability: packet.claimability.clone(),
            lifecycle: packet.lifecycle.clone(),
            proof_ladder_changes: packet.proof_ladder_changes.clone(),
            stale_unsafe_blockers: packet.stale_unsafe_blockers.clone(),
            validation_rules_evaluated: packet.validation_rules_evaluated.clone(),
            validation_rules_skipped: packet.validation_rules_skipped.clone(),
            aggregate_counts: hard_interrupt_error_aggregate_counts(&errors),
            warnings,
            unknowns,
            diagnostics,
            error_count: errors.len(),
            errors,
            omitted_count: packet.omitted_count,
            expansion_handles,
            generated_at: generated_at.into(),
        })
    }
}

fn hard_interrupt_sort_errors(errors: &mut [HardInterruptError]) {
    errors.sort_by(|left, right| hard_interrupt_error_order(left, right));
}

fn hard_interrupt_error_order(
    left: &HardInterruptError,
    right: &HardInterruptError,
) -> std::cmp::Ordering {
    left.severity
        .cmp(&right.severity)
        .then_with(|| left.validation_rule_id.cmp(&right.validation_rule_id))
        .then_with(|| left.file.cmp(&right.file))
        .then_with(|| {
            left.source_span
                .start_line
                .cmp(&right.source_span.start_line)
        })
        .then_with(|| {
            left.source_span
                .start_column
                .cmp(&right.source_span.start_column)
        })
        .then_with(|| left.source_span.end_line.cmp(&right.source_span.end_line))
        .then_with(|| {
            left.source_span
                .end_column
                .cmp(&right.source_span.end_column)
        })
        .then_with(|| left.error_id.cmp(&right.error_id))
}

fn hard_interrupt_compact_error_json(error: &HardInterruptError) -> Value {
    json!({
        "validation_rule_id": &error.validation_rule_id,
        "classification": error.classification,
        "blocking_level": error.blocking_level,
        "relation_kind": error.relation_kind,
        "integrity_kind": &error.integrity_kind,
        "file": &error.file,
        "source_span": &error.source_span,
        "symbol": &error.symbol,
        "target": &error.target,
        "source_symbol_summary": hard_interrupt_symbol_summary_json(&error.source_symbol_summary),
        "missing_target_summary": hard_interrupt_symbol_summary_json(&error.missing_target_summary),
        "message": &error.message,
        "exact_proof_reason": &error.exact_proof_reason,
        "recommended_fix": &error.recommended_fix,
        "suggested_next_steps": &error.suggested_next_steps,
        "reverified_graph_source_proof": error.reverified_graph_source_proof,
        "claimability": hard_interrupt_compact_claimability_json(&error.claimability),
        "lifecycle": hard_interrupt_lifecycle_state_summary_json(&error.lifecycle),
        "expansion_handle": &error.expansion_handle,
    })
}

fn hard_interrupt_compact_source_finding_json(finding: &InterruptSourceFinding) -> Value {
    json!({
        "finding_id": &finding.finding_id,
        "validation_rule_id": &finding.validation_rule_id,
        "classification": finding.classification,
        "blocking_level": finding.blocking_level,
        "file": &finding.file,
        "source_span": &finding.source_span,
        "relation_kind": finding.relation_kind,
        "integrity_kind": &finding.integrity_kind,
        "reason": &finding.reason,
        "recommended_fix": &finding.recommended_fix,
        "suggested_next_steps": &finding.suggested_next_steps,
        "expansion_handle": &finding.expansion_handle,
    })
}

fn hard_interrupt_symbol_summary_json(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return value.clone();
    };
    let mut compact = serde_json::Map::new();
    for key in [
        "symbol",
        "name",
        "qualified_name",
        "display",
        "file",
        "missing_target",
        "reason",
    ] {
        if let Some(field) = object.get(key) {
            compact.insert(key.to_string(), field.clone());
        }
    }
    Value::Object(compact)
}

fn hard_interrupt_source_findings_summary(
    findings: &[InterruptSourceFinding],
    top_limit: usize,
) -> Value {
    json!({
        "count": findings.len(),
        "top": findings
            .iter()
            .take(top_limit)
            .map(hard_interrupt_compact_source_finding_json)
            .collect::<Vec<_>>(),
        "omitted_count": findings.len().saturating_sub(top_limit),
        "counts_by_validation_rule_id": hard_interrupt_source_finding_counts_by_rule(findings),
        "counts_by_relation_kind": hard_interrupt_source_finding_counts_by_relation(findings),
        "counts_by_file": hard_interrupt_source_finding_counts_by_file(findings),
    })
}

fn hard_interrupt_source_finding_counts_by_rule(
    findings: &[InterruptSourceFinding],
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for finding in findings {
        *counts
            .entry(finding.validation_rule_id.clone())
            .or_default() += 1;
    }
    counts
}

fn hard_interrupt_source_finding_counts_by_relation(
    findings: &[InterruptSourceFinding],
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for finding in findings {
        let key = hard_interrupt_relation_kind_label(finding.relation_kind)
            .unwrap_or_else(|| "none".to_string());
        *counts.entry(key).or_default() += 1;
    }
    counts
}

fn hard_interrupt_source_finding_counts_by_file(
    findings: &[InterruptSourceFinding],
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for finding in findings {
        let key = finding
            .file
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        *counts.entry(key).or_default() += 1;
    }
    counts
}

fn hard_interrupt_error_aggregate_counts(errors: &[HardInterruptError]) -> Value {
    let mut by_rule_id = BTreeMap::<String, usize>::new();
    let mut by_relation_kind = BTreeMap::<String, usize>::new();
    let mut by_file = BTreeMap::<String, usize>::new();
    for error in errors {
        *by_rule_id
            .entry(error.validation_rule_id.clone())
            .or_default() += 1;
        let relation = hard_interrupt_relation_kind_label(error.relation_kind)
            .or_else(|| error.integrity_kind.clone())
            .unwrap_or_else(|| "none".to_string());
        *by_relation_kind.entry(relation).or_default() += 1;
        *by_file.entry(error.file.clone()).or_default() += 1;
    }
    json!({
        "by_validation_rule_id": by_rule_id,
        "by_relation_kind": by_relation_kind,
        "by_file": by_file,
    })
}

fn hard_interrupt_relation_kind_label(relation_kind: Option<RelationKind>) -> Option<String> {
    relation_kind.map(|relation| {
        serde_json::to_value(relation)
            .ok()
            .and_then(|value| value.as_str().map(ToString::to_string))
            .unwrap_or_else(|| format!("{relation:?}"))
    })
}

fn hard_interrupt_compact_claimability_json(value: &Value) -> Value {
    hard_interrupt_compact_object_json(
        value,
        &[
            "claimable",
            "current",
            "diagnostic_only",
            "candidate_only",
            "graph_proof_available",
            "safe",
            "packet_claim_state",
            "claimability_label",
        ],
    )
}

fn hard_interrupt_compact_lifecycle_json(value: &Value) -> Value {
    let mut compact = hard_interrupt_compact_object_json(
        value,
        &[
            "claimable",
            "current",
            "stale",
            "foreign",
            "schema_mismatched",
            "dirty",
            "partial",
            "diagnostic_only",
            "safe",
            "preflight_safe",
            "schema_status",
            "passport_status",
            "claimability_label",
        ],
    );
    if let (Some(source), Some(target)) = (value.as_object(), compact.as_object_mut()) {
        if let Some(blockers) = source.get("blockers").and_then(Value::as_array) {
            target.insert("blocker_count".to_string(), json!(blockers.len()));
        }
    }
    compact
}

fn hard_interrupt_lifecycle_state_summary_json(lifecycle: &ValidationLifecycleState) -> Value {
    json!({
        "claimable": lifecycle.claimable,
        "current": lifecycle.current,
        "stale": lifecycle.stale,
        "foreign": lifecycle.foreign,
        "schema_mismatched": lifecycle.schema_mismatched,
        "dirty": lifecycle.dirty,
        "partial": lifecycle.partial,
        "non_claimable_reason": &lifecycle.non_claimable_reason,
    })
}

fn hard_interrupt_compact_object_json(value: &Value, allowed_keys: &[&str]) -> Value {
    let Some(object) = value.as_object() else {
        return value.clone();
    };
    let mut compact = serde_json::Map::new();
    for key in allowed_keys {
        if let Some(field) = object.get(*key) {
            compact.insert((*key).to_string(), field.clone());
        }
    }
    Value::Object(compact)
}

fn hard_interrupt_compact_proof_ladder_changes_json(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return value.clone();
    };
    let mut compact = serde_json::Map::new();
    for (key, item) in object {
        if item.is_boolean() || item.is_number() || item.is_string() || item.is_null() {
            compact.insert(key.clone(), item.clone());
            continue;
        }
        if let Some(item_object) = item.as_object() {
            let mut item_summary = serde_json::Map::new();
            for allowed in [
                "changed",
                "status",
                "claimable",
                "diagnostic_only",
                "reason",
            ] {
                if let Some(field) = item_object.get(allowed) {
                    item_summary.insert(allowed.to_string(), field.clone());
                }
            }
            if !item_summary.is_empty() {
                compact.insert(key.clone(), Value::Object(item_summary));
            }
        } else if let Some(items) = item.as_array() {
            compact.insert(key.clone(), json!({"count": items.len()}));
        }
    }
    Value::Object(compact)
}

fn hard_interrupt_compact_rules_json(rules: &[ValidationRule]) -> Value {
    let mut sorted_rules = rules.iter().collect::<Vec<_>>();
    sorted_rules.sort_by(|left, right| left.validation_rule_id.cmp(&right.validation_rule_id));
    Value::Array(
        sorted_rules
            .into_iter()
            .map(|rule| {
                json!({
                    "validation_rule_id": &rule.validation_rule_id,
                    "rule_kind": rule.rule_kind,
                    "relation_kind": rule.relation_kind,
                    "invariant": &rule.invariant,
                    "activation_condition": &rule.activation_condition,
                    "proof_requirement": rule.proof_requirement,
                    "source_span_requirement": rule.source_span_requirement,
                    "provenance_requirement": rule.provenance_requirement,
                    "source_role_requirement": rule.source_role_requirement,
                    "lifecycle_requirement": rule.lifecycle_requirement,
                    "supported_relation_status": rule.supported_relation_status,
                    "default_classification_when_unsupported": rule.default_classification_when_unsupported,
                })
            })
            .collect(),
    )
}

fn hard_interrupt_compact_source_packet_summary(summary: Option<&Value>) -> Value {
    let Some(Value::Object(object)) = summary else {
        return Value::Null;
    };
    let mut compact = serde_json::Map::new();
    for key in [
        "schema_version",
        "packet_kind",
        "status",
        "must_fix_before_continuing",
        "hard_interrupt_available",
        "blocking_error_count",
        "warning_count",
        "unknown_count",
        "diagnostic_count",
    ] {
        if let Some(value) = object.get(key) {
            compact.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(compact)
}

fn hard_interrupt_compact_expansion_handles(
    sorted_errors: &[&HardInterruptError],
    error_limit: usize,
    omitted_count: usize,
) -> Vec<InterruptExpansionHandle> {
    let mut handles = Vec::new();
    let mut seen = BTreeMap::<String, ()>::new();
    for error in sorted_errors.iter().take(error_limit) {
        let handle = error.expansion_handle.clone();
        if seen.insert(handle.handle.clone(), ()).is_none() {
            handles.push(handle);
        }
    }
    if omitted_count > 0
        && seen
            .insert("hard_interrupt_packet:full".to_string(), ())
            .is_none()
    {
        handles.push(InterruptExpansionHandle {
            handle: "hard_interrupt_packet:full".to_string(),
            target: "hard_interrupt_packet".to_string(),
            reason: "Full hard-interrupt packet detail omitted from compact output.".to_string(),
        });
    }
    handles
}

impl HardInterruptError {
    pub fn from_validation_finding(
        finding: &ValidationFinding,
        rule: &ValidationRule,
        eligibility: InterruptEligibility,
    ) -> Option<Self> {
        if !eligibility.eligible {
            return None;
        }
        let file = finding
            .file
            .clone()
            .or_else(|| finding.affected_file.clone())
            .or_else(|| {
                finding
                    .source_span
                    .as_ref()
                    .map(|span| crate::normalize_repo_relative_path(&span.repo_relative_path))
            })?;
        let source_span = finding.source_span.clone().or_else(|| {
            (hard_interrupt_allows_missing_source_span(rule)
                || matches!(rule.rule_kind, ValidationRuleKind::LifecycleIntegrity))
            .then(|| SourceSpan::new(&file, 1, 1))
        })?;
        let rendered = hard_interrupt_rendered_message(finding, rule, &file, &source_span);
        let expansion_handle = InterruptExpansionHandle {
            handle: finding
                .expansion_handle
                .clone()
                .unwrap_or_else(|| format!("hard_interrupt:error:{}", finding.finding_id)),
            target: format!("validation_packet.finding:{}", finding.finding_id),
            reason:
                "Expand the source validation finding and graph/source evidence for this interrupt."
                    .to_string(),
        };

        Some(Self {
            error_id: format!("hard_interrupt_error:{}", finding.finding_id),
            validation_rule_id: finding.validation_rule_id.clone(),
            source_finding_id: finding.finding_id.clone(),
            classification: finding.classification,
            blocking_level: finding.blocking_level,
            rule_kind: rule.rule_kind,
            relation_kind: finding.relation_kind,
            integrity_kind: finding.integrity_kind.clone(),
            invariant: finding.invariant.clone(),
            severity: "blocking_graph_error".to_string(),
            file: file.clone(),
            source_span: source_span.clone(),
            symbol: finding
                .affected_entity
                .get("source")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .or(rendered.symbol),
            source_symbol_summary: rendered.source_symbol_summary,
            target: rendered.target,
            missing_target_summary: rendered.missing_target_summary,
            message: rendered.message,
            template_kind: rendered.template_kind,
            exact_proof_reason: rendered.exact_proof_reason,
            reverified_graph_source_proof: finding.reverified_graph_source_proof,
            source_role: finding.source_role,
            exactness: finding.exactness,
            provenance: finding.provenance.clone(),
            claimability: json!({
                "claimable": finding.lifecycle.claimable,
                "current": finding.lifecycle.current,
                "packet_claim_state": finding.new_fact_claim_state,
            }),
            lifecycle: finding.lifecycle.clone(),
            proof_strength: finding.proof_strength.clone(),
            proof_status: finding.proof_status,
            recommended_fix: rendered.recommended_fix.clone(),
            suggested_next_steps: rendered.suggested_next_steps.clone(),
            evidence_items: finding.evidence_items.clone(),
            expansion_handle: expansion_handle.clone(),
            eligibility,
            fix_hint: InterruptFixHint {
                recommended_fix: rendered.recommended_fix,
                suggested_next_steps: rendered.suggested_next_steps,
            },
        })
    }
}

#[derive(Debug, Clone)]
struct RenderedHardInterruptMessage {
    template_kind: String,
    symbol: Option<String>,
    target: Option<String>,
    source_symbol_summary: Value,
    missing_target_summary: Value,
    message: String,
    exact_proof_reason: String,
    recommended_fix: String,
    suggested_next_steps: Vec<String>,
}

fn hard_interrupt_rendered_message(
    finding: &ValidationFinding,
    rule: &ValidationRule,
    file: &str,
    source_span: &SourceSpan,
) -> RenderedHardInterruptMessage {
    let template_kind = hard_interrupt_template_kind(finding, rule);
    let relation_label = hard_interrupt_relation_label(finding, rule);
    let source_location = hard_interrupt_source_location_label(finding, source_span, file);
    let symbol = hard_interrupt_source_symbol(finding);
    let target = hard_interrupt_target_symbol(finding);
    let symbol_phrase = symbol
        .as_deref()
        .map(|value| format!(" from `{value}`"))
        .unwrap_or_default();
    let target_phrase = target
        .as_deref()
        .map(|value| format!(" `{value}`"))
        .unwrap_or_else(|| " the recorded target".to_string());
    let exact_proof_reason = finding.reason.clone();
    let proof_clause = hard_interrupt_proof_clause(&exact_proof_reason);
    let recommended_fix = hard_interrupt_recommended_fix(&template_kind, finding);
    let message = match template_kind.as_str() {
        "dangling_exact_calls" => format!(
            "Rule `{}` invariant `{}`: Exact {relation_label} relation at {source_location}{symbol_phrase} now points to missing target{target_phrase} after graph/source recheck. Restore the target or update the call, then rerun CodeGraph validation.{proof_clause}",
            finding.validation_rule_id, finding.invariant
        ),
        "dangling_exact_imports" => format!(
            "Rule `{}` invariant `{}`: Exact {relation_label} relation at {source_location}{symbol_phrase} now points to missing import target{target_phrase} after graph/source recheck. Restore the export/module or update the import, then rerun CodeGraph validation.{proof_clause}",
            finding.validation_rule_id, finding.invariant
        ),
        "missing_source_span" => format!(
            "Graph-integrity rule `{}` invariant `{}` found a claimable {relation_label} graph fact for {} without a current source span. Regenerate the fact from source before continuing.{proof_clause}",
            finding.validation_rule_id,
            finding.invariant,
            symbol
                .as_deref()
                .map(|value| format!("`{value}`"))
                .unwrap_or_else(|| format!("`{file}`"))
        ),
        "derived_missing_provenance" => format!(
            "Graph-integrity rule `{}` invariant `{}` found a derived {relation_label} graph fact at {source_location} without provenance. Attach provenance or downgrade the fact before continuing.{proof_clause}",
            finding.validation_rule_id, finding.invariant
        ),
        "source_role_leakage" => format!(
            "Source-role rule `{}` invariant `{}` found {relation_label} proof at {source_location}{symbol_phrase} using {} evidence as production proof. Move the proof path to production evidence or downgrade it.{proof_clause}",
            finding.validation_rule_id,
            finding.invariant,
            finding
                .source_role
                .map(hard_interrupt_source_role_label)
                .unwrap_or("non-production")
        ),
        "lifecycle_mismatch" => format!(
            "Lifecycle rule `{}` invariant `{}` reports an unsafe validation DB state for `{file}`. Refresh or rebuild the production agent-use DB before using this validation as proof.{proof_clause}",
            finding.validation_rule_id, finding.invariant
        ),
        "activation_gated_relation" => format!(
            "Rule `{}` invariant `{}`: Activated exact {relation_label} relation at {source_location}{symbol_phrase} now points to target{target_phrase} that failed graph/source validation. Restore or update the exact target, then rerun CodeGraph validation.{proof_clause}",
            finding.validation_rule_id, finding.invariant
        ),
        _ => format!(
            "Validation rule `{}` invariant `{}` reports a blocking graph validation finding at {source_location}. Apply the recommended fix, then rerun CodeGraph validation.{proof_clause}",
            finding.validation_rule_id, finding.invariant
        ),
    };

    let suggested_next_steps = hard_interrupt_suggested_next_steps(
        &template_kind,
        finding,
        file,
        &source_location,
        symbol.as_deref(),
        target.as_deref(),
        &recommended_fix,
    );

    RenderedHardInterruptMessage {
        source_symbol_summary: json!({
            "symbol": symbol,
            "file": file,
            "source_span": source_span,
            "source_role": finding.source_role,
            "template_kind": template_kind,
        }),
        missing_target_summary: json!({
            "target": target,
            "relation_kind": finding.relation_kind.or(rule.relation_kind),
            "integrity_kind": finding.integrity_kind,
            "affected_edge": finding.affected_edge,
            "affected_delta": finding.affected_delta,
            "template_kind": template_kind,
        }),
        template_kind,
        symbol,
        target,
        message,
        exact_proof_reason,
        recommended_fix,
        suggested_next_steps,
    }
}

fn hard_interrupt_proof_clause(reason: &str) -> String {
    let trimmed = reason.trim();
    if trimmed.is_empty() {
        " Proof: graph validation marked this finding as blocking.".to_string()
    } else if matches!(trimmed.chars().last(), Some('.') | Some('!') | Some('?')) {
        format!(" Proof: {trimmed}")
    } else {
        format!(" Proof: {trimmed}.")
    }
}

fn hard_interrupt_template_kind(finding: &ValidationFinding, rule: &ValidationRule) -> String {
    let rule_id = finding.validation_rule_id.to_ascii_uppercase();
    if rule_id.contains("SOURCE_SPAN") || finding.invariant.to_ascii_lowercase().contains("span") {
        "missing_source_span".to_string()
    } else if rule_id.contains("PROVENANCE")
        || finding
            .invariant
            .to_ascii_lowercase()
            .contains("provenance")
    {
        "derived_missing_provenance".to_string()
    } else if matches!(rule.rule_kind, ValidationRuleKind::SourceRoleBoundary)
        || rule_id.contains("SOURCE_ROLE")
        || rule_id.contains("TARGET_ROLE")
        || rule_id.contains("TEST_EVIDENCE")
        || rule_id.contains("MOCK")
        || rule_id.contains("STUB")
        || rule_id.contains("GENERATED")
    {
        "source_role_leakage".to_string()
    } else if matches!(rule.rule_kind, ValidationRuleKind::LifecycleIntegrity)
        || rule_id.contains("LIFECYCLE")
        || rule_id.contains("TRANSACTION")
        || rule_id.contains("QUERY_DURING_UPDATE")
    {
        "lifecycle_mismatch".to_string()
    } else if finding.relation_kind == Some(RelationKind::Calls) || rule_id.contains("CALLS") {
        "dangling_exact_calls".to_string()
    } else if matches!(
        finding.relation_kind.or(rule.relation_kind),
        Some(RelationKind::Imports | RelationKind::Exports | RelationKind::AliasedBy)
    ) || rule_id.contains("IMPORT")
    {
        "dangling_exact_imports".to_string()
    } else if matches!(
        finding.relation_kind.or(rule.relation_kind),
        Some(
            RelationKind::Reads
                | RelationKind::Writes
                | RelationKind::Handles
                | RelationKind::Configures
        )
    ) || rule_id.contains("READS")
        || rule_id.contains("WRITES")
        || rule_id.contains("ROUTE")
        || rule_id.contains("CONFIG")
    {
        "activation_gated_relation".to_string()
    } else {
        "generic".to_string()
    }
}

fn hard_interrupt_recommended_fix(template_kind: &str, finding: &ValidationFinding) -> String {
    match template_kind {
        "dangling_exact_calls" => {
            "Restore the missing target or update the call to an existing source-spanned target."
                .to_string()
        }
        "dangling_exact_imports" => {
            "Restore the exported symbol/module or update the import path/alias to the new target."
                .to_string()
        }
        "missing_source_span" => {
            "Regenerate the graph fact from source so the proof edge has a source span, or downgrade it to diagnostic if it cannot be source-spanned."
                .to_string()
        }
        "derived_missing_provenance" => {
            "Attach derivation provenance to the edge or remove/downgrade the derived edge from claimable proof."
                .to_string()
        }
        "source_role_leakage" => {
            "Move the proof path to production evidence, or mark the evidence as test/mock/stub/generated and keep it out of production proof."
                .to_string()
        }
        "lifecycle_mismatch" => {
            "Refresh or rebuild the production agent-use DB, then rerun validation. Do not use the current unsafe DB state as proof."
                .to_string()
        }
        "activation_gated_relation" => {
            "Restore or update the exact referenced target, or downgrade the relation if exact proof no longer exists."
                .to_string()
        }
        _ => finding.recommended_fix.clone().unwrap_or_else(|| {
            "Fix the blocking graph validation finding before continuing.".to_string()
        }),
    }
}

fn hard_interrupt_suggested_next_steps(
    template_kind: &str,
    finding: &ValidationFinding,
    file: &str,
    source_location: &str,
    symbol: Option<&str>,
    target: Option<&str>,
    recommended_fix: &str,
) -> Vec<String> {
    let mut steps = Vec::new();
    if finding.source_span.is_some() {
        steps.push(format!("Inspect {source_location}."));
    } else {
        steps.push(format!(
            "Inspect `{file}` and the source graph fact referenced by `{}`.",
            finding.validation_rule_id
        ));
    }
    if let Some(symbol) = symbol {
        steps.push(format!("Inspect source symbol `{symbol}`."));
    }
    if let Some(target) = target {
        steps.push(match template_kind {
            "dangling_exact_imports" => {
                format!("Restore or update imported/exported target `{target}`.")
            }
            "dangling_exact_calls" => format!("Restore or update CALLS target `{target}`."),
            _ => format!("Restore or update target `{target}`."),
        });
    }
    match template_kind {
        "missing_source_span" => {
            steps.push("Regenerate the graph fact from source, or downgrade it to diagnostic if a source span cannot be produced.".to_string());
        }
        "derived_missing_provenance" => {
            steps.push("Attach derivation provenance to the edge or remove/downgrade the derived proof fact.".to_string());
        }
        "source_role_leakage" => {
            steps.push("Move the proof path to production evidence or keep the non-production evidence out of production proof.".to_string());
        }
        "lifecycle_mismatch" => {
            steps.push("Refresh or rebuild the production agent-use DB before treating validation output as proof.".to_string());
        }
        _ => steps.push(recommended_fix.to_string()),
    }
    steps.push(format!(
        "Rerun `codegraph-mcp agent-use watch --repo <repo> --once --changed {file} --json`."
    ));
    hard_interrupt_dedup_steps(steps)
}

fn hard_interrupt_dedup_steps(steps: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for step in steps {
        if !deduped.iter().any(|existing| existing == &step) {
            deduped.push(step);
        }
    }
    deduped
}

fn hard_interrupt_relation_label(finding: &ValidationFinding, rule: &ValidationRule) -> String {
    finding
        .relation_kind
        .or(rule.relation_kind)
        .and_then(|kind| {
            serde_json::to_value(kind)
                .ok()
                .and_then(|value| value.as_str().map(ToString::to_string))
        })
        .unwrap_or_else(|| validation_rule_kind_label(rule.rule_kind).to_string())
}

fn hard_interrupt_source_location_label(
    finding: &ValidationFinding,
    source_span: &SourceSpan,
    file: &str,
) -> String {
    if let Some(span) = &finding.source_span {
        span.to_string()
    } else if finding.affected_file.is_some() || finding.file.is_some() {
        format!("`{file}`")
    } else {
        source_span.to_string()
    }
}

fn hard_interrupt_source_role_label(role: EvidenceRole) -> &'static str {
    match role {
        EvidenceRole::Production => "production",
        EvidenceRole::Test => "test",
        EvidenceRole::Mock => "mock",
        EvidenceRole::Mixed => "mixed",
        EvidenceRole::Unknown => "unknown",
    }
}

fn hard_interrupt_source_symbol(finding: &ValidationFinding) -> Option<String> {
    hard_interrupt_find_endpoint_display(&finding.affected_delta, "source_endpoint")
        .or_else(|| hard_interrupt_find_endpoint_display(&finding.affected_edge, "source_endpoint"))
        .or_else(|| {
            hard_interrupt_find_string_for_keys(
                &finding.affected_edge,
                &["caller", "importing_entity", "source_entity_id", "source"],
            )
        })
}

fn hard_interrupt_target_symbol(finding: &ValidationFinding) -> Option<String> {
    hard_interrupt_find_endpoint_display(&finding.affected_delta, "target_endpoint")
        .or_else(|| hard_interrupt_find_endpoint_display(&finding.affected_edge, "target_endpoint"))
        .or_else(|| hard_interrupt_display_from_value(&finding.affected_entity))
        .or_else(|| {
            hard_interrupt_find_string_for_keys(
                &finding.affected_edge,
                &[
                    "callee",
                    "import_target",
                    "target_entity_id",
                    "target",
                    "entity_id",
                ],
            )
        })
}

fn hard_interrupt_find_endpoint_display(value: &Value, endpoint_key: &str) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(endpoint) = map.get(endpoint_key) {
                if let Some(display) = hard_interrupt_display_from_value(endpoint) {
                    return Some(display);
                }
            }
            for child in map.values() {
                if let Some(display) = hard_interrupt_find_endpoint_display(child, endpoint_key) {
                    return Some(display);
                }
            }
            None
        }
        Value::Array(items) => items
            .iter()
            .find_map(|item| hard_interrupt_find_endpoint_display(item, endpoint_key)),
        _ => None,
    }
}

fn hard_interrupt_find_string_for_keys(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(value) = map.get(*key).and_then(Value::as_str) {
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
            for child in map.values() {
                if let Some(display) = hard_interrupt_find_string_for_keys(child, keys) {
                    return Some(display);
                }
            }
            None
        }
        Value::Array(items) => items
            .iter()
            .find_map(|item| hard_interrupt_find_string_for_keys(item, keys)),
        _ => None,
    }
}

fn hard_interrupt_display_from_value(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return (!text.is_empty()).then(|| text.to_string());
    }
    let map = value.as_object()?;
    for key in ["qualified_name", "name", "entity_id", "id"] {
        if let Some(text) = map.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

pub fn interrupt_eligibility_for_finding(
    finding: &ValidationFinding,
    rule: Option<&ValidationRule>,
    claimability: &Value,
) -> InterruptEligibility {
    let mut disqualifiers = Vec::<String>::new();
    let mut push_disqualifier = |reason: &str| {
        if !disqualifiers.iter().any(|item| item == reason) {
            disqualifiers.push(reason.to_string());
        }
    };

    let classification_ok = finding.classification == ValidationClassification::Block
        && finding.blocking_level == ValidationBlockingLevel::Blocking;
    if !classification_ok {
        push_disqualifier("classification_not_blocking");
    }

    let Some(rule) = rule else {
        push_disqualifier("validation_rule_not_found");
        return InterruptEligibility {
            eligible: false,
            reason: "validation rule was not available for interrupt eligibility".to_string(),
            disqualifiers,
            lifecycle_ok: false,
            claimability_ok: false,
            classification_ok,
            reverified_graph_source_proof: false,
            source_span_ok: false,
            source_role_ok: false,
            provenance_ok: false,
            evidence_kind_ok: false,
        };
    };

    if !matches!(
        rule.supported_relation_status,
        SupportedRelationStatus::ExactBlockingCandidate
    ) {
        push_disqualifier("rule_family_not_exact_blocking_candidate");
    }

    let lifecycle_rule = matches!(rule.rule_kind, ValidationRuleKind::LifecycleIntegrity);
    let lifecycle_ok = finding.lifecycle.is_claimable_current() || lifecycle_rule;
    if !lifecycle_ok {
        push_disqualifier("lifecycle_not_claimable_current");
    }

    let claimability_ok = lifecycle_rule || validation_claimability_allows_interrupt(claimability);
    if !claimability_ok {
        push_disqualifier("packet_claimability_does_not_permit_graph_source_proof");
    }

    let reverified_graph_source_proof = finding.reverified_graph_source_proof || lifecycle_rule;
    if !reverified_graph_source_proof {
        push_disqualifier("finding_not_reverified_graph_source_or_integrity_proof");
    }

    let source_span_ok = if lifecycle_rule || hard_interrupt_allows_missing_source_span(rule) {
        true
    } else if rule.source_span_requirement.requires_span() {
        finding.source_span.is_some()
    } else {
        true
    };
    if !source_span_ok {
        push_disqualifier("required_source_span_missing");
    }

    let source_role_ok = match (
        rule.rule_kind,
        rule.source_role_requirement,
        finding.source_role,
    ) {
        (ValidationRuleKind::SourceRoleBoundary, _, _) => true,
        (_, ValidationSourceRoleRequirement::ProductionOnlyByDefault, Some(role)) => {
            role.is_production()
        }
        _ => true,
    };
    if !source_role_ok {
        push_disqualifier("source_role_not_interrupt_compatible");
    }

    let provenance_required = rule.provenance_requirement.requires_provenance();
    let provenance_present = finding
        .provenance
        .get("present")
        .and_then(Value::as_bool)
        .unwrap_or(!provenance_required);
    let provenance_ok = if matches!(rule.rule_kind, ValidationRuleKind::ProofIntegrity)
        && finding
            .validation_rule_id
            .to_ascii_lowercase()
            .contains("provenance")
    {
        true
    } else {
        !provenance_required || provenance_present
    };
    if !provenance_ok {
        push_disqualifier("required_provenance_missing");
    }

    let evidence_kind_ok = finding
        .evidence_items
        .iter()
        .any(ValidationEvidenceItem::can_support_blocking_graph_proof)
        || (lifecycle_rule
            && finding
                .evidence_items
                .iter()
                .any(|item| item.evidence_kind == ValidationEvidenceKind::Lifecycle));
    if !evidence_kind_ok {
        push_disqualifier("evidence_kind_not_graph_source_or_integrity_proof");
    }

    if matches!(
        finding.proof_status,
        ValidationProofStatus::NotGraphProof
            | ValidationProofStatus::StaleOrNonClaimableDb
            | ValidationProofStatus::UnsupportedRelation
            | ValidationProofStatus::OverBudgetDegraded
            | ValidationProofStatus::NeedsReverification
            | ValidationProofStatus::Unknown
    ) {
        push_disqualifier("proof_status_not_interrupt_eligible");
    }

    if finding.proof_strength == "degraded_budget"
        || finding.proof_strength == "unsupported_relation"
        || finding.proof_strength == "diagnostic_only"
        || finding.proof_strength == "unknown"
        || finding.proof_strength == "not_graph_proof"
    {
        push_disqualifier("proof_strength_not_interrupt_eligible");
    }

    let eligible = disqualifiers.is_empty();
    InterruptEligibility {
        eligible,
        reason: if eligible {
            "finding is an MVP3.3 blocking graph/source or graph-integrity validation finding"
                .to_string()
        } else {
            "finding is not eligible for hard interrupt".to_string()
        },
        disqualifiers,
        lifecycle_ok,
        claimability_ok,
        classification_ok,
        reverified_graph_source_proof,
        source_span_ok,
        source_role_ok,
        provenance_ok,
        evidence_kind_ok,
    }
}

fn validation_claimability_allows_interrupt(claimability: &Value) -> bool {
    let claimable = claimability
        .get("claimable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let diagnostic_only = claimability
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let candidate_only = claimability
        .get("candidate_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let blockers_empty = claimability
        .get("blockers")
        .and_then(Value::as_array)
        .map(Vec::is_empty)
        .unwrap_or(true);
    let db_problem_absent = claimability
        .get("db_problem_kind")
        .map(Value::is_null)
        .unwrap_or(true);

    claimable && !diagnostic_only && !candidate_only && blockers_empty && db_problem_absent
}

fn hard_interrupt_allows_missing_source_span(rule: &ValidationRule) -> bool {
    matches!(rule.rule_kind, ValidationRuleKind::ProofIntegrity)
        && rule
            .validation_rule_id
            .to_ascii_lowercase()
            .contains("source_span")
}

pub fn classify_validation_finding(
    rule: &ValidationRule,
    finding_id: impl Into<String>,
    input: ValidationReverificationInput,
) -> ValidationFinding {
    let reverification = reverify_validation_graph_source_contract(&input);
    let classification = if reverification.over_budget {
        ValidationClassification::Degraded
    } else if reverification.unsupported_relation || !reverification.relation_supported {
        rule.default_classification_when_unsupported
    } else if !reverification.db_claimable_current {
        ValidationClassification::Diagnostic
    } else if !reverification.evidence_can_support_blocking {
        ValidationClassification::Diagnostic
    } else if reverification.blocking_integrity_condition_allowed
        || (reverification.blocking_graph_source_relation_allowed
            && matches!(
                rule.supported_relation_status,
                SupportedRelationStatus::ExactBlockingCandidate
            ))
    {
        ValidationClassification::Block
    } else if reverification.blocking_graph_source_relation_allowed
        && matches!(
            rule.supported_relation_status,
            SupportedRelationStatus::ExactWarningCandidate
        )
    {
        ValidationClassification::Warn
    } else if matches!(
        rule.supported_relation_status,
        SupportedRelationStatus::DiagnosticOnly
    ) {
        ValidationClassification::Diagnostic
    } else if matches!(
        rule.supported_relation_status,
        SupportedRelationStatus::Unsupported
    ) {
        ValidationClassification::Unsupported
    } else if matches!(
        rule.supported_relation_status,
        SupportedRelationStatus::ActivationGated | SupportedRelationStatus::Unknown
    ) || !reverification.graph_source_relation_reverified
        || !reverification.integrity_condition_reverified
    {
        ValidationClassification::Unknown
    } else {
        ValidationClassification::Diagnostic
    };

    let proof_status = proof_status_for_classification(&classification, &reverification);
    let proof_level = match proof_status {
        ValidationProofStatus::ReverifiedGraphSourceProof => "graph_source_reverified",
        ValidationProofStatus::ReverifiedGraphIntegrity => "graph_integrity_reverified",
        ValidationProofStatus::OverBudgetDegraded => "degraded",
        ValidationProofStatus::UnsupportedRelation => "unsupported",
        ValidationProofStatus::StaleOrNonClaimableDb => "non_claimable_lifecycle",
        ValidationProofStatus::NotGraphProof => "not_graph_proof",
        ValidationProofStatus::MissingRequiredSourceSpan => "missing_source_span",
        ValidationProofStatus::MissingRequiredProvenance => "missing_provenance",
        ValidationProofStatus::NeedsReverification => "needs_reverification",
        ValidationProofStatus::Unknown => "unknown",
    };
    let proof_strength = match classification {
        ValidationClassification::Block => "deterministic_graph_source",
        ValidationClassification::Warn => "exact_warning",
        ValidationClassification::Unsupported => "unsupported_relation",
        ValidationClassification::Degraded => "degraded_budget",
        ValidationClassification::Diagnostic => "diagnostic_only",
        ValidationClassification::Unknown => "unknown",
    };

    let mut unknowns = Vec::new();
    let mut diagnostics = Vec::new();
    if matches!(
        classification,
        ValidationClassification::Unknown | ValidationClassification::Unsupported
    ) {
        unknowns.push(
            reverification
                .downgrade_reason
                .clone()
                .unwrap_or_else(|| "proof_path_cannot_be_verified".to_string()),
        );
    }
    if !classification.is_blocking() {
        diagnostics.push(
            reverification
                .downgrade_reason
                .clone()
                .unwrap_or_else(|| "not_a_blocking_graph_source_finding".to_string()),
        );
    }

    ValidationFinding {
        finding_id: finding_id.into(),
        validation_rule_id: rule.validation_rule_id.clone(),
        invariant: rule.invariant.clone(),
        classification,
        blocking_level: classification.blocking_level(),
        integrity_kind: Some(validation_rule_kind_label(rule.rule_kind).to_string()),
        proof_level: proof_level.to_string(),
        proof_strength: proof_strength.to_string(),
        proof_status,
        affected_evidence: json!(&input.evidence_items),
        affected_delta: Value::Null,
        affected_edge: Value::Null,
        affected_entity: Value::Null,
        file: None,
        affected_file: None,
        source_span: None,
        source_role: None,
        relation_kind: rule.relation_kind,
        exactness: input.relation_exact.then_some(Exactness::ParserVerified),
        provenance: json!({
            "required": input.provenance_required,
            "present": input.provenance_present
        }),
        old_fact_claim_state: "unknown".to_string(),
        new_fact_claim_state: if reverification.db_claimable_current {
            "claimable_current".to_string()
        } else {
            "non_claimable".to_string()
        },
        lifecycle: input.lifecycle,
        reverified_graph_source_proof: reverification.blocking_graph_source_relation_allowed
            || reverification.blocking_integrity_condition_allowed,
        evidence_items: input.evidence_items,
        reason: input.reason,
        recommended_fix: None,
        suggested_next_steps: vec![rule.docs_summary.clone()],
        unknowns,
        diagnostics,
        expansion_handle: None,
    }
}

fn validation_rule_kind_label(kind: ValidationRuleKind) -> &'static str {
    match kind {
        ValidationRuleKind::DanglingTarget => "dangling_target",
        ValidationRuleKind::BrokenContract => "broken_contract",
        ValidationRuleKind::ProofIntegrity => "proof_integrity",
        ValidationRuleKind::SourceRoleBoundary => "source_role_boundary",
        ValidationRuleKind::LifecycleIntegrity => "lifecycle_integrity",
        ValidationRuleKind::UnsupportedRelationBoundary => "unsupported_relation_boundary",
        ValidationRuleKind::ClosureBudgetBoundary => "closure_budget_boundary",
        ValidationRuleKind::Diagnostic => "diagnostic",
    }
}

fn validation_classification_label(classification: ValidationClassification) -> &'static str {
    match classification {
        ValidationClassification::Block => "block",
        ValidationClassification::Warn => "warn",
        ValidationClassification::Unknown => "unknown",
        ValidationClassification::Unsupported => "unsupported",
        ValidationClassification::Degraded => "degraded",
        ValidationClassification::Diagnostic => "diagnostic",
    }
}

fn proof_status_for_classification(
    classification: &ValidationClassification,
    reverification: &ValidationReverification,
) -> ValidationProofStatus {
    if reverification.over_budget {
        return ValidationProofStatus::OverBudgetDegraded;
    }
    if reverification.unsupported_relation || !reverification.relation_supported {
        return ValidationProofStatus::UnsupportedRelation;
    }
    if !reverification.db_claimable_current {
        return ValidationProofStatus::StaleOrNonClaimableDb;
    }
    if reverification.source_span_required && !reverification.source_span_present {
        return ValidationProofStatus::MissingRequiredSourceSpan;
    }
    if reverification.provenance_required && !reverification.provenance_present {
        return ValidationProofStatus::MissingRequiredProvenance;
    }
    if !reverification.evidence_can_support_blocking {
        return ValidationProofStatus::NotGraphProof;
    }
    if matches!(classification, ValidationClassification::Block)
        && reverification.blocking_integrity_condition_allowed
    {
        return ValidationProofStatus::ReverifiedGraphIntegrity;
    }
    if reverification.blocking_graph_source_relation_allowed {
        return ValidationProofStatus::ReverifiedGraphSourceProof;
    }
    if !reverification.graph_source_relation_reverified
        && !reverification.integrity_condition_reverified
    {
        return ValidationProofStatus::NeedsReverification;
    }
    ValidationProofStatus::Unknown
}

fn validation_stale_unsafe_blockers_from_state(
    claimability: &Value,
    lifecycle: &Value,
) -> Vec<String> {
    let mut blockers = Vec::<String>::new();
    let mut push_blocker = |blocker: String| {
        if !blocker.trim().is_empty() && !blockers.contains(&blocker) {
            blockers.push(blocker);
        }
    };

    if let Some(items) = claimability.get("blockers").and_then(Value::as_array) {
        for item in items {
            if let Some(blocker) = item.as_str() {
                push_blocker(blocker.to_string());
            }
        }
    }
    if let Some(problem_kind) = claimability.get("db_problem_kind").and_then(Value::as_str) {
        push_blocker(problem_kind.to_string());
    }
    if claimability
        .get("diagnostic_only")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        push_blocker("diagnostic_only_claimability".to_string());
    }
    if let Some(reason) = lifecycle
        .get("non_claimable_reason")
        .and_then(Value::as_str)
    {
        push_blocker(reason.to_string());
    }
    for key in ["stale", "foreign", "schema_mismatched", "dirty", "partial"] {
        if lifecycle.get(key).and_then(Value::as_bool).unwrap_or(false) {
            push_blocker(key.to_string());
        }
    }
    blockers
}

fn validation_recommended_next_steps(
    blocking_errors: &[ValidationFinding],
    warnings: &[ValidationFinding],
    unknowns: &[ValidationFinding],
    diagnostics: &[ValidationFinding],
    stale_unsafe_blockers: &[String],
) -> Vec<String> {
    let mut steps = Vec::<String>::new();
    fn push_step(steps: &mut Vec<String>, step: String) {
        if !step.trim().is_empty() && !steps.contains(&step) {
            steps.push(step);
        }
    }

    for finding in blocking_errors {
        if let Some(fix) = finding.recommended_fix.clone() {
            push_step(&mut steps, fix);
        }
        for step in &finding.suggested_next_steps {
            push_step(&mut steps, step.clone());
        }
    }
    if !blocking_errors.is_empty() && steps.is_empty() {
        push_step(
            &mut steps,
            "Fix blocking graph validation findings before continuing.".to_string(),
        );
    }
    if !stale_unsafe_blockers.is_empty() {
        push_step(
            &mut steps,
            "Refresh or rebuild the agent-use index before relying on validation proof."
                .to_string(),
        );
    }
    if blocking_errors.is_empty()
        && (!warnings.is_empty() || !unknowns.is_empty() || !diagnostics.is_empty())
    {
        push_step(
            &mut steps,
            "Inspect warnings, unknowns, and diagnostics without treating them as blocking graph proof."
                .to_string(),
        );
    }
    if steps.is_empty() {
        push_step(
            &mut steps,
            "No MVP3.3 validation findings require action.".to_string(),
        );
    }
    steps.truncate(6);
    steps
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationPacket {
    pub schema_version: u32,
    pub packet_kind: ValidationPacketKind,
    pub status: ValidationPacketStatus,
    pub must_fix_before_continuing: bool,
    pub changed_files: Vec<String>,
    pub graph_delta: Value,
    pub blocking_errors: Vec<ValidationFinding>,
    pub warnings: Vec<ValidationFinding>,
    pub unknowns: Vec<ValidationFinding>,
    pub diagnostics: Vec<ValidationFinding>,
    pub summary_counts_by_rule_id: BTreeMap<String, usize>,
    pub summary_counts_by_classification: BTreeMap<String, usize>,
    pub summary_counts_by_relation_kind: BTreeMap<String, usize>,
    pub validation_rules_evaluated: Vec<ValidationRule>,
    pub validation_rules_skipped: Vec<ValidationRule>,
    pub relation_family_status: Value,
    pub activation_gate_state: Value,
    pub claimability: Value,
    pub proof_ladder_changes: Value,
    pub lifecycle: Value,
    pub stale_unsafe_blockers: Vec<String>,
    pub top_blocking_source_spans: Vec<SourceSpan>,
    pub recommended_next_steps: Vec<String>,
    pub hard_interrupt_available: bool,
    #[serde(default)]
    pub hard_interrupt: Option<HardInterruptPacket>,
    pub omitted_count: usize,
    pub expansion_handles: Vec<String>,
}

impl ValidationPacket {
    pub fn new(
        changed_files: Vec<String>,
        graph_delta: Value,
        findings: Vec<ValidationFinding>,
        validation_rules_evaluated: Vec<ValidationRule>,
        validation_rules_skipped: Vec<ValidationRule>,
        claimability: Value,
        proof_ladder_changes: Value,
        lifecycle: Value,
    ) -> Self {
        let mut blocking_errors = Vec::new();
        let mut warnings = Vec::new();
        let mut unknowns = Vec::new();
        let mut diagnostics = Vec::new();

        for mut finding in findings {
            finding.sync_agent_json_aliases();
            match finding.classification {
                ValidationClassification::Block => blocking_errors.push(finding),
                ValidationClassification::Warn | ValidationClassification::Degraded => {
                    warnings.push(finding)
                }
                ValidationClassification::Unknown | ValidationClassification::Unsupported => {
                    unknowns.push(finding)
                }
                ValidationClassification::Diagnostic => diagnostics.push(finding),
            }
        }

        let mut summary_counts_by_rule_id = BTreeMap::<String, usize>::new();
        let mut summary_counts_by_classification = BTreeMap::<String, usize>::new();
        let mut summary_counts_by_relation_kind = BTreeMap::<String, usize>::new();
        for finding in blocking_errors
            .iter()
            .chain(warnings.iter())
            .chain(unknowns.iter())
            .chain(diagnostics.iter())
        {
            *summary_counts_by_rule_id
                .entry(finding.validation_rule_id.clone())
                .or_default() += 1;
            *summary_counts_by_classification
                .entry(validation_classification_label(finding.classification).to_string())
                .or_default() += 1;
            if let Some(relation_kind) = finding.relation_kind {
                *summary_counts_by_relation_kind
                    .entry(relation_kind.to_string())
                    .or_default() += 1;
            }
        }

        let status = if !blocking_errors.is_empty() {
            ValidationPacketStatus::BlockingGraphError
        } else if !warnings.is_empty() {
            ValidationPacketStatus::Warning
        } else if !unknowns.is_empty() {
            ValidationPacketStatus::Unknown
        } else if !diagnostics.is_empty() {
            ValidationPacketStatus::DiagnosticOnly
        } else {
            ValidationPacketStatus::Ok
        };
        let stale_unsafe_blockers =
            validation_stale_unsafe_blockers_from_state(&claimability, &lifecycle);
        let top_blocking_source_spans = blocking_errors
            .iter()
            .filter_map(|finding| finding.source_span.clone())
            .take(3)
            .collect::<Vec<_>>();
        let recommended_next_steps = validation_recommended_next_steps(
            &blocking_errors,
            &warnings,
            &unknowns,
            &diagnostics,
            &stale_unsafe_blockers,
        );

        Self {
            schema_version: VALIDATION_PACKET_SCHEMA_VERSION,
            packet_kind: ValidationPacketKind::GraphValidationPacket,
            status,
            must_fix_before_continuing: !blocking_errors.is_empty(),
            changed_files: changed_files
                .into_iter()
                .map(crate::normalize_repo_relative_path)
                .collect(),
            graph_delta,
            blocking_errors,
            warnings,
            unknowns,
            diagnostics,
            summary_counts_by_rule_id,
            summary_counts_by_classification,
            summary_counts_by_relation_kind,
            validation_rules_evaluated,
            validation_rules_skipped,
            relation_family_status: json!({}),
            activation_gate_state: json!({}),
            claimability,
            proof_ladder_changes,
            lifecycle,
            stale_unsafe_blockers,
            top_blocking_source_spans,
            recommended_next_steps,
            hard_interrupt_available: false,
            hard_interrupt: None,
            omitted_count: 0,
            expansion_handles: Vec::new(),
        }
    }

    pub fn with_hard_interrupt_packet(mut self, hard_interrupt: HardInterruptPacket) -> Self {
        self.status = ValidationPacketStatus::BlockingGraphError;
        self.must_fix_before_continuing = true;
        self.hard_interrupt_available = true;
        self.hard_interrupt = Some(hard_interrupt);
        self
    }

    pub fn with_eligible_hard_interrupts(mut self, generated_at: impl Into<String>) -> Self {
        if let Some(hard_interrupt) =
            HardInterruptPacket::from_validation_packet(&self, generated_at)
        {
            self = self.with_hard_interrupt_packet(hard_interrupt);
        }
        self
    }

    pub const fn critical_safety_field_names() -> &'static [&'static str] {
        &[
            "status",
            "must_fix_before_continuing",
            "changed_files",
            "graph_delta",
            "blocking_errors",
            "warnings",
            "unknowns",
            "diagnostics",
            "summary_counts_by_rule_id",
            "summary_counts_by_classification",
            "summary_counts_by_relation_kind",
            "relation_family_status",
            "activation_gate_state",
            "claimability",
            "proof_ladder_changes",
            "lifecycle",
            "stale_unsafe_blockers",
            "top_blocking_source_spans",
            "recommended_next_steps",
            "hard_interrupt_available",
            "hard_interrupt",
            "omitted_count",
            "expansion_handles",
        ]
    }

    pub fn compact_agent_json(&self, max_findings_per_bucket: usize) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        let mut omitted = self.omitted_count;

        for key in ["blocking_errors", "warnings", "unknowns", "diagnostics"] {
            if let Some(items) = value.get_mut(key).and_then(Value::as_array_mut) {
                if items.len() > max_findings_per_bucket {
                    omitted += items.len() - max_findings_per_bucket;
                    items.truncate(max_findings_per_bucket);
                }
            }
        }

        if let (Some(object), Some(hard_interrupt)) = (value.as_object_mut(), &self.hard_interrupt)
        {
            object.insert(
                "hard_interrupt".to_string(),
                hard_interrupt.compact_agent_json(max_findings_per_bucket),
            );
        }

        let critical_safety_fields_preserved = Self::critical_safety_fields_preserved_in(&value);
        if let Some(object) = value.as_object_mut() {
            object.insert("omitted_count".to_string(), json!(omitted));
            object.insert(
                "critical_safety_fields_preserved".to_string(),
                json!(critical_safety_fields_preserved),
            );
            if omitted > 0 {
                let handles = if self.expansion_handles.is_empty() {
                    vec!["validation_packet:full".to_string()]
                } else {
                    self.expansion_handles.clone()
                };
                object.insert("expansion_handles".to_string(), json!(handles));
            }
        }

        value
    }

    pub fn critical_safety_fields_preserved_in(value: &Value) -> bool {
        Self::critical_safety_field_names()
            .iter()
            .all(|field| value.get(*field).is_some())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationEndpointClass {
    Any,
    Container,
    Declaration,
    TypeLike,
    Executable,
    Data,
    Security,
    Async,
    Persistence,
    Test,
    Evidence,
}

pub fn relation_allows(
    relation: RelationKind,
    head_kind: EntityKind,
    tail_kind: EntityKind,
) -> bool {
    endpoint_matches_any(head_kind, relation.domain_classes())
        && endpoint_matches_any(tail_kind, relation.codomain_classes())
}

fn endpoint_matches_any(kind: EntityKind, classes: &'static [RelationEndpointClass]) -> bool {
    classes.iter().any(|class| endpoint_matches(kind, *class))
}

fn endpoint_matches(kind: EntityKind, class: RelationEndpointClass) -> bool {
    match class {
        RelationEndpointClass::Any => true,
        RelationEndpointClass::Container => is_container(kind),
        RelationEndpointClass::Declaration => is_declaration(kind),
        RelationEndpointClass::TypeLike => is_type_like(kind),
        RelationEndpointClass::Executable => is_executable(kind),
        RelationEndpointClass::Data => is_data(kind),
        RelationEndpointClass::Security => is_security(kind),
        RelationEndpointClass::Async => is_async(kind),
        RelationEndpointClass::Persistence => is_persistence(kind),
        RelationEndpointClass::Test => is_test(kind),
        RelationEndpointClass::Evidence => is_evidence(kind),
    }
}

impl RelationKind {
    pub const fn domain_classes(self) -> &'static [RelationEndpointClass] {
        use RelationEndpointClass as C;
        match self {
            Self::Contains
            | Self::Defines
            | Self::Declares
            | Self::Exports
            | Self::Imports
            | Self::Reexports
            | Self::Configures => &[C::Container],
            Self::DefinedIn | Self::BelongsTo => &[C::Declaration, C::Executable, C::Data, C::Test],

            Self::TypeOf | Self::Returns => &[C::Data, C::Executable, C::Declaration],
            Self::Implements | Self::Extends | Self::Overrides => &[C::TypeLike],
            Self::Instantiates | Self::Injects => &[C::Executable, C::Declaration],
            Self::AliasedBy | Self::AliasOf => &[C::Declaration, C::Data, C::TypeLike],

            Self::Calls | Self::CalledBy | Self::Callee | Self::Spawns | Self::Awaits => {
                &[C::Executable]
            }
            Self::ReturnsTo => &[C::Executable, C::Data],
            Self::Argument0 | Self::Argument1 | Self::ArgumentN => &[C::Executable],

            Self::Reads | Self::Writes | Self::Mutates => &[C::Executable],
            Self::MutatedBy => &[C::Data, C::Persistence],
            Self::FlowsTo
            | Self::ReachingDef
            | Self::AssignedFrom
            | Self::ControlDependsOn
            | Self::DataDependsOn => &[C::Data, C::Executable],

            Self::Authorizes
            | Self::ChecksRole
            | Self::ChecksPermission
            | Self::Sanitizes
            | Self::Validates
            | Self::Exposes
            | Self::TrustBoundary
            | Self::SinksTo => &[C::Executable, C::Security],
            Self::SourceOfTaint => &[C::Data, C::Executable],

            Self::Publishes
            | Self::Emits
            | Self::Consumes
            | Self::ListensTo
            | Self::SubscribesTo
            | Self::Handles => &[C::Executable, C::Async],

            Self::Migrates
            | Self::ReadsTable
            | Self::WritesTable
            | Self::AltersColumn
            | Self::DependsOnSchema => &[C::Executable, C::Persistence],

            Self::Tests
            | Self::Asserts
            | Self::Mocks
            | Self::Stubs
            | Self::Covers
            | Self::FixturesFor => &[C::Test],

            Self::MayMutate | Self::MayRead | Self::ApiReaches => &[C::Executable, C::Security],
            Self::AsyncReaches => &[C::Executable, C::Async],
            Self::SchemaImpact => &[C::Executable, C::Persistence],
        }
    }

    pub const fn codomain_classes(self) -> &'static [RelationEndpointClass] {
        use RelationEndpointClass as C;
        match self {
            Self::Contains
            | Self::Defines
            | Self::Declares
            | Self::Exports
            | Self::Imports
            | Self::Reexports
            | Self::Configures => &[C::Any],
            Self::DefinedIn | Self::BelongsTo => &[C::Container],

            Self::TypeOf | Self::Returns => &[C::TypeLike, C::Data],
            Self::Implements | Self::Extends | Self::Overrides => &[C::TypeLike],
            Self::Instantiates | Self::Injects => &[C::TypeLike, C::Declaration],
            Self::AliasedBy | Self::AliasOf => &[C::Declaration, C::Data, C::TypeLike],

            Self::Calls
            | Self::CalledBy
            | Self::Callee
            | Self::ReturnsTo
            | Self::Spawns
            | Self::Awaits => &[C::Executable],
            Self::Argument0 | Self::Argument1 | Self::ArgumentN => &[C::Data, C::Declaration],

            Self::Reads | Self::Writes | Self::Mutates | Self::FlowsTo => {
                &[C::Data, C::Persistence, C::Executable]
            }
            Self::MutatedBy => &[C::Executable],
            Self::ReachingDef
            | Self::AssignedFrom
            | Self::ControlDependsOn
            | Self::DataDependsOn => &[C::Data, C::Executable],

            Self::Authorizes | Self::ChecksRole | Self::ChecksPermission => &[C::Security],
            Self::Sanitizes | Self::Validates | Self::SourceOfTaint | Self::SinksTo => {
                &[C::Data, C::Persistence]
            }
            Self::Exposes | Self::TrustBoundary => &[C::Security, C::Executable, C::Data],

            Self::Publishes | Self::Emits => &[C::Async],
            Self::Consumes | Self::ListensTo | Self::SubscribesTo | Self::Handles => {
                &[C::Async, C::Executable]
            }

            Self::Migrates
            | Self::ReadsTable
            | Self::WritesTable
            | Self::AltersColumn
            | Self::DependsOnSchema => &[C::Persistence],

            Self::Tests | Self::Covers => &[C::Declaration, C::Executable, C::Data, C::Persistence],
            Self::Asserts => &[C::Data, C::Executable],
            Self::Mocks | Self::Stubs | Self::FixturesFor => {
                &[C::Declaration, C::Executable, C::Data]
            }

            Self::MayMutate | Self::MayRead => &[C::Data, C::Persistence, C::Executable],
            Self::ApiReaches => &[C::Executable, C::Security, C::Data],
            Self::AsyncReaches => &[C::Executable, C::Async],
            Self::SchemaImpact => &[C::Data, C::Persistence],
        }
    }

    pub fn allows(self, head_kind: EntityKind, tail_kind: EntityKind) -> bool {
        relation_allows(self, head_kind, tail_kind)
    }
}

fn is_container(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Repository
            | EntityKind::Package
            | EntityKind::Module
            | EntityKind::Directory
            | EntityKind::File
            | EntityKind::Class
            | EntityKind::Interface
            | EntityKind::Trait
            | EntityKind::Enum
            | EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::Database
            | EntityKind::Table
            | EntityKind::TestFile
            | EntityKind::TestSuite
    )
}

fn is_declaration(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Class
            | EntityKind::Interface
            | EntityKind::Trait
            | EntityKind::Enum
            | EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::Parameter
            | EntityKind::Field
            | EntityKind::Property
            | EntityKind::Type
            | EntityKind::GenericType
            | EntityKind::Import
            | EntityKind::Export
            | EntityKind::Dependency
    )
}

fn is_type_like(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Type
            | EntityKind::GenericType
            | EntityKind::Class
            | EntityKind::Interface
            | EntityKind::Trait
            | EntityKind::Enum
    )
}

fn is_executable(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::CallSite
            | EntityKind::Route
            | EntityKind::Endpoint
            | EntityKind::Middleware
            | EntityKind::Job
            | EntityKind::Promise
            | EntityKind::Task
            | EntityKind::TestSuite
            | EntityKind::TestCase
    )
}

fn is_data(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Parameter
            | EntityKind::LocalVariable
            | EntityKind::GlobalVariable
            | EntityKind::Field
            | EntityKind::Property
            | EntityKind::Expression
            | EntityKind::Assignment
            | EntityKind::ReturnSite
            | EntityKind::ConfigKey
            | EntityKind::EnvVar
            | EntityKind::Column
            | EntityKind::Table
    )
}

fn is_security(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::AuthPolicy
            | EntityKind::Role
            | EntityKind::Permission
            | EntityKind::Sanitizer
            | EntityKind::Validator
            | EntityKind::Middleware
            | EntityKind::Route
            | EntityKind::Endpoint
    )
}

fn is_async(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Event
            | EntityKind::Topic
            | EntityKind::Queue
            | EntityKind::Job
            | EntityKind::Promise
            | EntityKind::Task
    )
}

fn is_persistence(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Database | EntityKind::Table | EntityKind::Column | EntityKind::Migration
    )
}

fn is_test(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::TestFile
            | EntityKind::TestSuite
            | EntityKind::TestCase
            | EntityKind::Fixture
            | EntityKind::Mock
            | EntityKind::Stub
            | EntityKind::Assertion
    )
}

fn is_evidence(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::PathEvidence | EntityKind::DerivedClosureEdge
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn exact_calls_rule() -> ValidationRule {
        ValidationRule::exact_blocking(
            "mvp3_3.calls.dangling_target",
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Calls),
            "exact CALLS edges must resolve to a claimable target",
            "Define the missing target or update the exact call relation.",
        )
    }

    fn exact_graph_source_input() -> ValidationReverificationInput {
        ValidationReverificationInput::exact_graph_source(
            ValidationLifecycleState::claimable_current(),
            "edge://calls",
            "exact CALLS edge was reverified from graph/source",
        )
    }

    fn exact_finding_with_span(rule: &ValidationRule, id: &str) -> ValidationFinding {
        let mut finding = classify_validation_finding(rule, id, exact_graph_source_input());
        finding.affected_file = Some("src/main.rs".to_string());
        finding.source_span = Some(SourceSpan::with_columns("src/main.rs", 1, 1, 1, 12));
        finding.recommended_fix = Some("Define the missing callee.".to_string());
        finding.suggested_next_steps = vec!["Define the missing callee.".to_string()];
        finding
    }

    fn hard_interrupt_eligibility_example() -> InterruptEligibility {
        InterruptEligibility {
            eligible: true,
            reason: "schema fixture: caller supplied an already reverified blocking graph/source finding"
                .to_string(),
            disqualifiers: Vec::new(),
            lifecycle_ok: true,
            claimability_ok: true,
            classification_ok: true,
            reverified_graph_source_proof: true,
            source_span_ok: true,
            source_role_ok: true,
            provenance_ok: true,
            evidence_kind_ok: true,
        }
    }

    fn hard_interrupt_expansion_handle_example() -> InterruptExpansionHandle {
        InterruptExpansionHandle {
            handle: "hard_interrupt:error:finding-calls".to_string(),
            target: "errors[0]".to_string(),
            reason: "Expand the top hard-interrupt error with full evidence detail.".to_string(),
        }
    }

    fn hard_interrupt_error_example() -> HardInterruptError {
        let rule = exact_calls_rule();
        let finding = exact_finding_with_span(&rule, "finding://calls/hard-interrupt");
        let file = finding
            .file
            .clone()
            .or_else(|| finding.affected_file.clone())
            .unwrap_or_else(|| "src/main.rs".to_string());
        let source_span = finding
            .source_span
            .clone()
            .unwrap_or_else(|| SourceSpan::new(&file, 1, 1));
        let recommended_fix = finding
            .recommended_fix
            .clone()
            .unwrap_or_else(|| "Define the missing callee.".to_string());
        let suggested_next_steps = if finding.suggested_next_steps.is_empty() {
            vec![recommended_fix.clone()]
        } else {
            finding.suggested_next_steps.clone()
        };
        let expansion_handle = hard_interrupt_expansion_handle_example();

        HardInterruptError {
            error_id: "hard-interrupt-error://calls/missing-callee".to_string(),
            validation_rule_id: finding.validation_rule_id.clone(),
            source_finding_id: finding.finding_id.clone(),
            classification: ValidationClassification::Block,
            blocking_level: ValidationBlockingLevel::Blocking,
            rule_kind: rule.rule_kind,
            relation_kind: finding.relation_kind,
            integrity_kind: finding.integrity_kind.clone(),
            invariant: finding.invariant.clone(),
            severity: "blocking_graph_error".to_string(),
            file: file.clone(),
            source_span,
            symbol: Some("caller".to_string()),
            source_symbol_summary: json!({
                "symbol": "caller",
                "file": file
            }),
            target: None,
            missing_target_summary: json!({
                "missing_target": "missing_callee",
                "reason": "exact CALLS edge points at a missing target"
            }),
            message: "Exact CALLS edge points at a missing target.".to_string(),
            template_kind: "dangling_exact_calls".to_string(),
            exact_proof_reason: finding.reason.clone(),
            reverified_graph_source_proof: true,
            source_role: finding.source_role,
            exactness: finding.exactness,
            provenance: finding.provenance.clone(),
            claimability: json!({"claimable": true, "current": true}),
            lifecycle: finding.lifecycle.clone(),
            proof_strength: finding.proof_strength.clone(),
            proof_status: finding.proof_status,
            recommended_fix: recommended_fix.clone(),
            suggested_next_steps: suggested_next_steps.clone(),
            evidence_items: finding.evidence_items.clone(),
            expansion_handle: expansion_handle.clone(),
            eligibility: hard_interrupt_eligibility_example(),
            fix_hint: InterruptFixHint {
                recommended_fix,
                suggested_next_steps,
            },
        }
    }

    fn hard_interrupt_packet_example() -> HardInterruptPacket {
        let rule = exact_calls_rule();
        let error = hard_interrupt_error_example();
        let expansion_handle = error.expansion_handle.clone();
        HardInterruptPacket {
            schema_version: HARD_INTERRUPT_PACKET_SCHEMA_VERSION,
            packet_kind: HardInterruptPacketKind::HardInterrupt,
            status: ValidationPacketStatus::BlockingGraphError,
            must_fix_before_continuing: true,
            hard_interrupt_available: true,
            changed_files: vec!["src/main.rs".to_string()],
            source_validation_packet_ref: None,
            source_validation_packet_summary: Some(json!({
                "packet_kind": "graph_validation_packet",
                "status": "blocking_graph_error",
                "must_fix_before_continuing": true,
                "hard_interrupt_available": true,
                "blocking_error_count": 1
            })),
            source_validation_packet_kind: ValidationPacketKind::GraphValidationPacket,
            errors: vec![error.clone()],
            warnings: Vec::new(),
            unknowns: Vec::new(),
            diagnostics: Vec::new(),
            error_count: 1,
            warnings_summary: json!({
                "count": 0,
                "top": [],
                "omitted_count": 0,
                "counts_by_validation_rule_id": {},
                "counts_by_relation_kind": {},
                "counts_by_file": {},
            }),
            unknowns_summary: json!({
                "count": 0,
                "top": [],
                "omitted_count": 0,
                "counts_by_validation_rule_id": {},
                "counts_by_relation_kind": {},
                "counts_by_file": {},
            }),
            diagnostics_summary: json!({
                "count": 0,
                "top": [],
                "omitted_count": 0,
                "counts_by_validation_rule_id": {},
                "counts_by_relation_kind": {},
                "counts_by_file": {},
            }),
            summary: InterruptPacketSummary {
                error_count: 1,
                warning_count: 0,
                unknown_count: 0,
                diagnostic_count: 0,
                top_error_validation_rule_id: Some(error.validation_rule_id.clone()),
                top_error_file: Some(error.file.clone()),
                top_error_source_span: Some(error.source_span.clone()),
                top_error_recommended_fix: Some(error.recommended_fix.clone()),
                top_error_suggested_next_steps: error.suggested_next_steps.clone(),
                critical_safety_fields_preserved: true,
            },
            claimability: json!({"claimable": true, "current": true}),
            lifecycle: json!({"claimable": true, "current": true}),
            proof_ladder_changes: json!({"graph_source_proof": {"changed": true}}),
            stale_unsafe_blockers: Vec::new(),
            validation_rules_evaluated: vec![rule],
            validation_rules_skipped: Vec::new(),
            aggregate_counts: hard_interrupt_error_aggregate_counts(&[error.clone()]),
            omitted_count: 0,
            expansion_handles: vec![expansion_handle],
            generated_at: "2026-06-03T00:00:00Z".to_string(),
        }
    }

    fn schema_root_for_test() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("docs")
            .join("schemas")
            .join("agent-json")
    }

    fn read_agent_json_schema_for_test(schema_name: &str) -> Value {
        let path = schema_root_for_test().join(schema_name);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read schema {}: {error}", path.display()));
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("parse schema {}: {error}", path.display()))
    }

    fn claimable_interrupt_packet_for(
        rule: ValidationRule,
        findings: Vec<ValidationFinding>,
    ) -> ValidationPacket {
        ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"edge_delta_count": findings.len()}}),
            findings,
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true, "current": true, "blockers": []}),
            serde_json::json!({"graph_relation_proof": {"changed": true}}),
            serde_json::json!({"claimable": true, "current": true}),
        )
        .with_eligible_hard_interrupts("test-generated-at")
    }

    fn exact_imports_rule() -> ValidationRule {
        ValidationRule::exact_blocking(
            "mvp3_3.imports.dangling_target",
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Imports),
            "exact IMPORTS edges must resolve to a claimable target",
            "Define the missing import target or update the exact import relation.",
        )
    }

    fn proof_integrity_source_span_rule() -> ValidationRule {
        let mut rule = ValidationRule::exact_blocking(
            "mvp3_3.proof.source_span_missing",
            ValidationRuleKind::ProofIntegrity,
            Some(RelationKind::Calls),
            "claimable graph facts must carry source spans",
            "Regenerate the graph fact with a valid source span or downgrade the fact.",
        );
        rule.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
        rule
    }

    fn proof_integrity_provenance_rule() -> ValidationRule {
        let mut rule = ValidationRule::exact_blocking(
            "mvp3_3.proof.derived_missing_provenance",
            ValidationRuleKind::ProofIntegrity,
            Some(RelationKind::Calls),
            "derived graph facts must carry provenance",
            "Attach provenance edges or downgrade the derived fact.",
        );
        rule.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
        rule.provenance_requirement = ValidationProvenanceRequirement::RequiredForDerivedEdges;
        rule
    }

    fn source_role_boundary_rule() -> ValidationRule {
        ValidationRule::exact_blocking(
            "mvp3_3.source_role.test_evidence",
            ValidationRuleKind::SourceRoleBoundary,
            Some(RelationKind::Calls),
            "production proof must not rely on test evidence",
            "Move the proof to production source or downgrade the evidence.",
        )
    }

    fn activation_gated_reads_rule() -> ValidationRule {
        let mut rule = ValidationRule::exact_blocking(
            "mvp3_3.reads.dangling_symbol",
            ValidationRuleKind::DanglingTarget,
            Some(RelationKind::Reads),
            "exact READS edges must resolve to a current symbol entity",
            "Restore the read symbol or update the READS relation.",
        );
        rule.activation_condition =
            "READS relation is already activated and emitted as an MVP3.3 block".to_string();
        rule
    }

    fn lifecycle_integrity_rule() -> ValidationRule {
        let mut rule = ValidationRule::exact_blocking(
            "mvp3_3.lifecycle.db_mismatch",
            ValidationRuleKind::LifecycleIntegrity,
            None,
            "production agent-use DB lifecycle must be safe before validation can block",
            "Refresh or rebuild the production agent-use DB.",
        );
        rule.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
        rule.source_span_requirement = ValidationSourceSpanRequirement::NotApplicable;
        rule
    }

    fn generic_blocking_rule() -> ValidationRule {
        ValidationRule::exact_blocking(
            "mvp3_3.generic.blocking_contract",
            ValidationRuleKind::BrokenContract,
            None,
            "generic blocking contract must be repaired before continuing",
            "Repair the generic blocking contract.",
        )
    }

    fn block_finding_for_rule(rule: &ValidationRule, id: &str) -> ValidationFinding {
        exact_finding_with_span(rule, id)
    }

    fn hard_interrupt_error_for(
        rule: ValidationRule,
        mut finding: ValidationFinding,
    ) -> HardInterruptError {
        finding.sync_agent_json_aliases();
        let eligibility = interrupt_eligibility_for_finding(
            &finding,
            Some(&rule),
            &serde_json::json!({"claimable": true, "current": true, "blockers": []}),
        );
        assert!(eligibility.eligible, "{eligibility:?}");
        HardInterruptError::from_validation_finding(&finding, &rule, eligibility)
            .expect("hard interrupt error")
    }

    fn finding_with_endpoint_names(
        rule: &ValidationRule,
        id: &str,
        source: &str,
        target: &str,
    ) -> ValidationFinding {
        let mut finding = block_finding_for_rule(rule, id);
        finding.affected_delta = json!({
            "source_endpoint": {
                "name": source,
                "qualified_name": format!("fixture::{source}")
            },
            "target_endpoint": {
                "name": target,
                "qualified_name": format!("fixture::{target}")
            }
        });
        finding
    }

    fn hard_interrupt_packet_with_error_count(count: usize) -> HardInterruptPacket {
        let rule = exact_calls_rule();
        let findings = (0..count)
            .map(|index| {
                let mut finding = finding_with_endpoint_names(
                    &rule,
                    &format!("finding://calls/compact/{index}"),
                    &format!("caller_{index}"),
                    &format!("missing_{index}"),
                );
                let file = format!("src/file_{index:02}.rs");
                finding.file = Some(file.clone());
                finding.affected_file = Some(file.clone());
                finding.source_span = Some(SourceSpan::with_columns(
                    &file,
                    (index + 1) as u32,
                    1,
                    (index + 1) as u32,
                    12,
                ));
                finding
            })
            .collect::<Vec<_>>();
        claimable_interrupt_packet_for(rule, findings)
            .hard_interrupt
            .expect("hard interrupt packet")
    }

    fn non_interrupt_packet_for(
        mut finding: ValidationFinding,
        rule: ValidationRule,
    ) -> ValidationPacket {
        finding.sync_agent_json_aliases();
        claimable_interrupt_packet_for(rule, vec![finding])
    }

    #[test]
    fn blocking_finding_requires_reverified_graph_source_proof() {
        let rule = exact_calls_rule();
        let finding =
            classify_validation_finding(&rule, "finding://calls/1", exact_graph_source_input());
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(finding.blocking_level, ValidationBlockingLevel::Blocking);
        assert!(finding.reverified_graph_source_proof);

        let mut input = exact_graph_source_input();
        input.graph_source_relation_reverified = false;
        let downgraded =
            classify_validation_finding(&rule, "finding://calls/not-reverified", input);
        assert_ne!(downgraded.classification, ValidationClassification::Block);
        assert_eq!(downgraded.blocking_level, ValidationBlockingLevel::Unknown);
        assert!(!downgraded.reverified_graph_source_proof);
    }

    #[test]
    fn candidate_text_vector_and_source_navigation_evidence_cannot_block() {
        let rule = exact_calls_rule();
        for evidence_kind in [
            ValidationEvidenceKind::TextEvidence,
            ValidationEvidenceKind::Candidate,
            ValidationEvidenceKind::Vector,
            ValidationEvidenceKind::SourceNavigation,
        ] {
            let mut input = exact_graph_source_input();
            input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                evidence_kind,
                format!("evidence://{evidence_kind:?}"),
                "non-graph evidence cannot prove graph/source failure",
            )];
            let finding =
                classify_validation_finding(&rule, format!("finding://{evidence_kind:?}"), input);
            assert_ne!(finding.classification, ValidationClassification::Block);
            assert_eq!(finding.classification, ValidationClassification::Diagnostic);
            assert!(!finding.reverified_graph_source_proof);
        }
    }

    #[test]
    fn stale_db_state_prevents_blocking_proof() {
        let rule = exact_calls_rule();
        let mut input = exact_graph_source_input();
        input.lifecycle = ValidationLifecycleState::stale_non_claimable("repo_head_mismatch");

        let finding = classify_validation_finding(&rule, "finding://stale-db", input);

        assert_eq!(finding.classification, ValidationClassification::Diagnostic);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::StaleOrNonClaimableDb
        );
        assert_eq!(
            finding.blocking_level,
            ValidationBlockingLevel::DiagnosticOnly
        );
        assert!(!finding.reverified_graph_source_proof);
    }

    #[test]
    fn derived_edge_without_provenance_blocks_only_when_claimable_integrity_is_reverified() {
        let mut rule = ValidationRule::exact_blocking(
            "mvp3_3.integrity.derived_provenance",
            ValidationRuleKind::ProofIntegrity,
            Some(RelationKind::Mutates),
            "derived edges must carry provenance before they are claimable proof",
            "Attach provenance edges or downgrade the derived fact.",
        );
        rule.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
        rule.provenance_requirement = ValidationProvenanceRequirement::RequiredForDerivedEdges;

        let mut input = exact_graph_source_input();
        input.graph_source_relation_reverified = false;
        input.integrity_condition_reverified = true;
        input.provenance_required = true;
        input.provenance_present = false;
        input.evidence_items = vec![ValidationEvidenceItem::graph_integrity(
            "edge://derived-without-provenance",
            "claimable derived edge is missing required provenance",
        )];
        let finding =
            classify_validation_finding(&rule, "finding://missing-provenance", input.clone());
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::MissingRequiredProvenance
        );
        assert!(finding.reverified_graph_source_proof);

        input.lifecycle = ValidationLifecycleState::stale_non_claimable("partial_update");
        let stale = classify_validation_finding(&rule, "finding://missing-provenance/stale", input);
        assert_ne!(stale.classification, ValidationClassification::Block);
        assert!(!stale.reverified_graph_source_proof);
    }

    #[test]
    fn unsupported_relation_class_maps_to_unsupported_or_unknown_not_blocking() {
        let mut rule = exact_calls_rule();
        rule.supported_relation_status = SupportedRelationStatus::Unsupported;
        rule.default_classification_when_unsupported = ValidationClassification::Unsupported;

        let mut input = exact_graph_source_input();
        input.relation_supported = false;
        input.unsupported_relation = true;

        let finding = classify_validation_finding(&rule, "finding://unsupported", input);
        assert!(matches!(
            finding.classification,
            ValidationClassification::Unsupported | ValidationClassification::Unknown
        ));
        assert_ne!(finding.blocking_level, ValidationBlockingLevel::Blocking);
    }

    #[test]
    fn over_budget_closure_maps_to_degraded_not_blocking() {
        let rule = exact_calls_rule();
        let mut input = exact_graph_source_input();
        input.over_budget = true;

        let finding = classify_validation_finding(&rule, "finding://over-budget", input);

        assert_eq!(finding.classification, ValidationClassification::Degraded);
        assert_eq!(finding.blocking_level, ValidationBlockingLevel::Warning);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::OverBudgetDegraded
        );
    }

    #[test]
    fn missing_source_span_maps_to_integrity_finding_when_claimable_fact_requires_span() {
        let mut rule = exact_calls_rule();
        rule.proof_requirement = ValidationProofRequirement::ReverifiedGraphIntegrity;
        rule.source_span_requirement =
            ValidationSourceSpanRequirement::RequiredForClaimableGraphFact;

        let mut input = exact_graph_source_input();
        input.graph_source_relation_reverified = false;
        input.integrity_condition_reverified = true;
        input.source_span_required = true;
        input.source_span_present = false;
        input.evidence_items = vec![ValidationEvidenceItem::graph_integrity(
            "edge://spanless-proof-edge",
            "claimable graph fact requires a source span",
        )];

        let finding = classify_validation_finding(&rule, "finding://missing-span", input);
        assert_eq!(finding.classification, ValidationClassification::Block);
        assert_eq!(
            finding.proof_status,
            ValidationProofStatus::MissingRequiredSourceSpan
        );
        assert!(finding.reverified_graph_source_proof);
    }

    #[test]
    fn validation_packet_includes_required_fields() {
        let rule = exact_calls_rule();
        let finding = classify_validation_finding(
            &rule,
            "finding://calls/packet",
            exact_graph_source_input(),
        );
        let packet = ValidationPacket::new(
            vec!["src\\main.rs".to_string()],
            serde_json::json!({"summary": {"edge_delta_count": 1}}),
            vec![finding],
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({"graph_relation_proof": {"changed": true}}),
            serde_json::json!({"claimable": true, "current": true}),
        );

        let value = serde_json::to_value(&packet).expect("packet serializes");
        for field in [
            "packet_kind",
            "status",
            "must_fix_before_continuing",
            "changed_files",
            "graph_delta",
            "blocking_errors",
            "warnings",
            "unknowns",
            "diagnostics",
            "summary_counts_by_rule_id",
            "summary_counts_by_classification",
            "summary_counts_by_relation_kind",
            "validation_rules_evaluated",
            "validation_rules_skipped",
            "relation_family_status",
            "activation_gate_state",
            "claimability",
            "proof_ladder_changes",
            "lifecycle",
            "stale_unsafe_blockers",
            "top_blocking_source_spans",
            "recommended_next_steps",
            "hard_interrupt_available",
            "omitted_count",
            "expansion_handles",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
        assert_eq!(
            value["packet_kind"].as_str(),
            Some("graph_validation_packet")
        );
        assert_eq!(value["changed_files"][0].as_str(), Some("src/main.rs"));
        assert_eq!(value["must_fix_before_continuing"].as_bool(), Some(true));
        assert_eq!(value["hard_interrupt_available"].as_bool(), Some(false));
        assert!(value["recommended_next_steps"].as_array().is_some());
    }

    #[test]
    fn validation_packet_integration_complete() {
        let rule = exact_calls_rule();
        let finding = exact_finding_with_span(&rule, "finding://calls/integration");
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"entity_delta_count": 1}, "omitted_count": 0}),
            vec![finding],
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({"graph_relation_proof": {"changed": true}}),
            serde_json::json!({"claimable": true, "current": true}),
        );
        let value = serde_json::to_value(&packet).expect("packet serializes");

        for field in ValidationPacket::critical_safety_field_names() {
            assert!(
                value.get(*field).is_some(),
                "missing critical field {field}"
            );
        }
        let finding = &value["blocking_errors"][0];
        for field in [
            "validation_rule_id",
            "invariant",
            "classification",
            "blocking_level",
            "relation_kind",
            "integrity_kind",
            "affected_evidence",
            "affected_delta",
            "file",
            "source_span",
            "source_role",
            "exactness",
            "provenance",
            "reverified_graph_source_proof",
            "proof_strength",
            "proof_status",
            "reason",
            "recommended_fix",
            "suggested_next_steps",
            "unknowns",
            "diagnostics",
        ] {
            assert!(
                finding.get(field).is_some(),
                "missing finding field {field}"
            );
        }
        assert_eq!(finding["file"].as_str(), Some("src/main.rs"));
        assert_eq!(
            value["summary_counts_by_classification"]["block"].as_u64(),
            Some(1)
        );
    }

    #[test]
    fn blocking_errors_source_spanned() {
        let rule = exact_calls_rule();
        let finding = exact_finding_with_span(&rule, "finding://calls/source-span");
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {}}),
            vec![finding],
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );
        let value = serde_json::to_value(packet).expect("packet serializes");
        assert_eq!(
            value["blocking_errors"][0]["file"].as_str(),
            Some("src/main.rs")
        );
        assert!(value["blocking_errors"][0]["source_span"].is_object());
        assert_eq!(
            value["top_blocking_source_spans"][0]["repo_relative_path"].as_str(),
            Some("src/main.rs")
        );
    }

    #[test]
    fn warnings_unknowns_diagnostics_present() {
        let rule = exact_calls_rule();
        let mut degraded_input = exact_graph_source_input();
        degraded_input.over_budget = true;
        let degraded =
            classify_validation_finding(&rule, "finding://warning/degraded", degraded_input);

        let mut unsupported_rule = rule.clone();
        unsupported_rule.supported_relation_status = SupportedRelationStatus::Unsupported;
        unsupported_rule.default_classification_when_unsupported =
            ValidationClassification::Unsupported;
        let mut unsupported_input = exact_graph_source_input();
        unsupported_input.relation_supported = false;
        unsupported_input.unsupported_relation = true;
        let unsupported = classify_validation_finding(
            &unsupported_rule,
            "finding://unknown/unsupported",
            unsupported_input,
        );

        let mut diagnostic_input = exact_graph_source_input();
        diagnostic_input.graph_source_relation_reverified = false;
        diagnostic_input.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::Candidate,
            "candidate://diagnostic",
            "candidate evidence is not graph proof",
        )];
        let diagnostic =
            classify_validation_finding(&rule, "finding://diagnostic/candidate", diagnostic_input);

        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {}}),
            vec![degraded, unsupported, diagnostic],
            vec![rule, unsupported_rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );

        assert_eq!(packet.blocking_errors.len(), 0);
        assert_eq!(packet.warnings.len(), 1);
        assert_eq!(packet.unknowns.len(), 1);
        assert_eq!(packet.diagnostics.len(), 1);
    }

    #[test]
    fn compact_output_preserves_critical_fields() {
        let rule = exact_calls_rule();
        let mut findings = Vec::new();
        for index in 0..4 {
            let mut input = exact_graph_source_input();
            input.graph_source_relation_reverified = false;
            input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                ValidationEvidenceKind::Candidate,
                format!("candidate://{index}"),
                "candidate evidence remains non-proof",
            )];
            findings.push(classify_validation_finding(
                &rule,
                format!("finding://candidate/{index}"),
                input,
            ));
        }
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"entity_delta_count": 1}}),
            findings,
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );

        let compact = packet.compact_agent_json(1);
        assert!(ValidationPacket::critical_safety_fields_preserved_in(
            &compact
        ));
        assert_eq!(
            compact["critical_safety_fields_preserved"].as_bool(),
            Some(true)
        );
        assert!(compact["omitted_count"].as_u64().unwrap_or_default() > 0);
        assert!(compact["expansion_handles"].as_array().is_some());
        assert!(compact["diagnostics"].as_array().unwrap().len() <= 1);
    }

    #[test]
    fn compact_packet_budget_enforced() {
        let rule = exact_calls_rule();
        let findings = (0..5)
            .map(|index| {
                let mut input = exact_graph_source_input();
                input.graph_source_relation_reverified = false;
                input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                    ValidationEvidenceKind::Candidate,
                    format!("candidate://{index}"),
                    "candidate evidence remains non-proof",
                )];
                classify_validation_finding(&rule, format!("finding://candidate/{index}"), input)
            })
            .collect::<Vec<_>>();
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"entity_delta_count": 5}}),
            findings,
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );
        let compact = packet.compact_agent_json(2);
        assert!(compact["omitted_count"].as_u64().unwrap_or_default() >= 3);
        assert!(compact["diagnostics"].as_array().unwrap().len() <= 2);
        assert!(compact["expansion_handles"].as_array().is_some());
    }

    #[test]
    fn explain_audit_restores_detail() {
        let interrupt = hard_interrupt_packet_with_error_count(4);
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {}}),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            serde_json::json!({"claimable": true, "current": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        )
        .with_hard_interrupt_packet(interrupt);
        let full = serde_json::to_value(&packet).expect("packet serializes");
        let compact = packet.compact_agent_json(1);
        let full_interrupt = &full["hard_interrupt"];
        let compact_interrupt = &compact["hard_interrupt"];
        assert_eq!(full_interrupt["errors"].as_array().unwrap().len(), 4);
        assert_eq!(compact_interrupt["errors"].as_array().unwrap().len(), 1);
        assert!(full_interrupt["errors"][0]["evidence_items"].is_array());
        assert!(full_interrupt["errors"][0]["eligibility"].is_object());
        assert!(full_interrupt["errors"][0]["provenance"].is_object());
        assert!(compact_interrupt["errors"][0]
            .get("evidence_items")
            .is_none());
        assert!(compact_interrupt["errors"][0].get("eligibility").is_none());
        assert!(compact_interrupt["errors"][0].get("provenance").is_none());
        assert!(
            compact_interrupt["omitted_count"]
                .as_u64()
                .unwrap_or_default()
                >= 3
        );
    }

    #[test]
    fn critical_safety_fields_preserved_under_truncation() {
        let rule = exact_calls_rule();
        let findings = (0..4)
            .map(|index| exact_finding_with_span(&rule, &format!("finding://blocking/{index}")))
            .collect::<Vec<_>>();
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"edge_delta_count": 4}}),
            findings,
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({"graph_relation_proof": {"changed": true}}),
            serde_json::json!({"claimable": true, "current": true}),
        );
        let compact = packet.compact_agent_json(1);
        assert!(ValidationPacket::critical_safety_fields_preserved_in(
            &compact
        ));
        assert_eq!(
            compact["critical_safety_fields_preserved"].as_bool(),
            Some(true)
        );
        assert_eq!(compact["must_fix_before_continuing"].as_bool(), Some(true));
        assert!(
            compact["top_blocking_source_spans"]
                .as_array()
                .unwrap()
                .len()
                <= 3
        );
    }

    #[test]
    fn hard_interrupt_not_implemented() {
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({}),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true}),
        );

        let value = serde_json::to_value(packet).expect("packet serializes");
        assert_eq!(
            value["packet_kind"].as_str(),
            Some("graph_validation_packet")
        );
        assert_eq!(value["hard_interrupt_available"].as_bool(), Some(false));
        assert!(value["hard_interrupt"].is_null());
    }

    #[test]
    fn hard_interrupt_packet_schema_defined() {
        let packet = hard_interrupt_packet_example();
        let value = serde_json::to_value(packet).expect("hard-interrupt packet serializes");

        assert_eq!(value["schema_version"].as_u64(), Some(1));
        assert_eq!(value["packet_kind"].as_str(), Some("hard_interrupt"));
        assert_eq!(value["status"].as_str(), Some("blocking_graph_error"));
        assert_eq!(value["must_fix_before_continuing"].as_bool(), Some(true));
        assert_eq!(value["hard_interrupt_available"].as_bool(), Some(true));
        assert_eq!(
            value["source_validation_packet_kind"].as_str(),
            Some("graph_validation_packet")
        );
        assert_eq!(value["errors"].as_array().unwrap().len(), 1);
        assert!(value["summary"].is_object());
        assert!(value["claimability"].is_object());
        assert!(value["lifecycle"].is_object());
        assert!(value["proof_ladder_changes"].is_object());
    }

    #[test]
    fn hard_interrupt_error_schema_defined() {
        let error = hard_interrupt_error_example();
        let value = serde_json::to_value(error).expect("hard-interrupt error serializes");
        for field in [
            "error_id",
            "validation_rule_id",
            "source_finding_id",
            "classification",
            "blocking_level",
            "rule_kind",
            "relation_kind",
            "integrity_kind",
            "invariant",
            "severity",
            "file",
            "source_span",
            "symbol",
            "source_symbol_summary",
            "target",
            "missing_target_summary",
            "message",
            "exact_proof_reason",
            "reverified_graph_source_proof",
            "source_role",
            "exactness",
            "provenance",
            "claimability",
            "lifecycle",
            "proof_strength",
            "proof_status",
            "recommended_fix",
            "suggested_next_steps",
            "evidence_items",
            "expansion_handle",
            "eligibility",
            "fix_hint",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
        assert_eq!(value["classification"].as_str(), Some("block"));
        assert_eq!(value["blocking_level"].as_str(), Some("blocking"));
        assert_eq!(value["reverified_graph_source_proof"].as_bool(), Some(true));
    }

    #[test]
    fn interrupt_eligibility_schema_defined() {
        let value = serde_json::to_value(hard_interrupt_eligibility_example())
            .expect("interrupt eligibility serializes");
        for field in [
            "eligible",
            "reason",
            "disqualifiers",
            "lifecycle_ok",
            "claimability_ok",
            "classification_ok",
            "reverified_graph_source_proof",
            "source_span_ok",
            "source_role_ok",
            "provenance_ok",
            "evidence_kind_ok",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
        assert_eq!(value["eligible"].as_bool(), Some(true));
    }

    #[test]
    fn validation_packet_compatibility_preserved() {
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {}}),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );
        let value = serde_json::to_value(&packet).expect("validation packet serializes");
        assert_eq!(
            value["packet_kind"].as_str(),
            Some("graph_validation_packet")
        );
        assert_eq!(value["hard_interrupt_available"].as_bool(), Some(false));
        assert!(value["hard_interrupt"].is_null());

        let reparsed: ValidationPacket =
            serde_json::from_value(value).expect("validation packet reparses");
        assert!(!reparsed.hard_interrupt_available);
        assert!(reparsed.hard_interrupt.is_none());
    }

    #[test]
    fn validation_packet_without_interrupt_still_valid() {
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"edge_delta_count": 0}}),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );

        assert_eq!(packet.status, ValidationPacketStatus::Ok);
        assert!(!packet.must_fix_before_continuing);
        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
        let compact = packet.compact_agent_json(1);
        assert!(ValidationPacket::critical_safety_fields_preserved_in(
            &compact
        ));
        assert_eq!(compact["hard_interrupt_available"].as_bool(), Some(false));
        assert!(compact["hard_interrupt"].is_null());
    }

    #[test]
    fn validation_packet_with_interrupt_still_valid() {
        let rule = exact_calls_rule();
        let finding = exact_finding_with_span(&rule, "finding://calls/with-interrupt");
        let validation_packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"edge_delta_count": 1}}),
            vec![finding],
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({"graph_source_proof": {"changed": true}}),
            serde_json::json!({"claimable": true, "current": true}),
        )
        .with_hard_interrupt_packet(hard_interrupt_packet_example());

        let value = serde_json::to_value(&validation_packet).expect("packet serializes");
        assert_eq!(value["hard_interrupt_available"].as_bool(), Some(true));
        assert!(value["hard_interrupt"].is_object());
        assert_eq!(value["blocking_errors"].as_array().map(Vec::len), Some(1));

        let reparsed: ValidationPacket =
            serde_json::from_value(value).expect("packet with interrupt reparses");
        assert!(reparsed.hard_interrupt_available);
        assert!(reparsed.hard_interrupt.is_some());
        assert_eq!(reparsed.blocking_errors.len(), 1);
    }

    #[test]
    fn required_safety_fields_present() {
        let compact = hard_interrupt_packet_example().compact_agent_json(1);
        assert!(HardInterruptPacket::critical_safety_fields_preserved_in(
            &compact
        ));
        assert_eq!(
            compact["critical_safety_fields_preserved"].as_bool(),
            Some(true)
        );
        assert_eq!(compact["summary"]["error_count"].as_u64(), Some(1));
        assert_eq!(
            compact["summary"]["top_error_validation_rule_id"].as_str(),
            Some("mvp3_3.calls.dangling_target")
        );
        assert_eq!(
            compact["summary"]["top_error_file"].as_str(),
            Some("src/main.rs")
        );
        assert!(compact["summary"]["top_error_source_span"].is_object());
        assert!(compact["summary"]["top_error_recommended_fix"].is_string());
        assert!(compact["summary"]["top_error_suggested_next_steps"].is_array());
    }

    #[test]
    fn hard_interrupt_compact_default() {
        let compact = hard_interrupt_packet_with_error_count(2).compact_agent_json(1);
        for field in [
            "packet_kind",
            "status",
            "must_fix_before_continuing",
            "hard_interrupt_available",
            "changed_files",
            "errors",
            "error_count",
            "warnings_summary",
            "unknowns_summary",
            "diagnostics_summary",
            "claimability",
            "lifecycle",
            "stale_unsafe_blockers",
            "proof_ladder_changes",
            "validation_rules_evaluated",
            "validation_rules_skipped",
            "omitted_count",
            "expansion_handles",
        ] {
            assert!(
                compact.get(field).is_some(),
                "missing compact field {field}"
            );
        }
        assert_eq!(compact["errors"].as_array().unwrap().len(), 1);
        assert_eq!(compact["error_count"].as_u64(), Some(2));
        let error = &compact["errors"][0];
        for field in [
            "validation_rule_id",
            "classification",
            "blocking_level",
            "relation_kind",
            "integrity_kind",
            "file",
            "source_span",
            "symbol",
            "target",
            "source_symbol_summary",
            "missing_target_summary",
            "message",
            "exact_proof_reason",
            "recommended_fix",
            "suggested_next_steps",
            "reverified_graph_source_proof",
            "claimability",
            "lifecycle",
            "expansion_handle",
        ] {
            assert!(
                error.get(field).is_some(),
                "missing compact error field {field}"
            );
        }
        for forbidden in [
            "error_id",
            "source_finding_id",
            "rule_kind",
            "invariant",
            "severity",
            "template_kind",
            "provenance",
            "proof_strength",
            "proof_status",
            "evidence_items",
            "eligibility",
            "fix_hint",
        ] {
            assert!(
                error.get(forbidden).is_none(),
                "compact error leaked full-detail field {forbidden}"
            );
        }
    }

    #[test]
    fn critical_fields_preserved_under_truncation() {
        let compact = hard_interrupt_packet_with_error_count(5).compact_agent_json(1);
        assert!(HardInterruptPacket::critical_safety_fields_preserved_in(
            &compact
        ));
        assert_eq!(
            compact["critical_safety_fields_preserved"].as_bool(),
            Some(true)
        );
        assert_eq!(compact["must_fix_before_continuing"].as_bool(), Some(true));
        assert_eq!(compact["hard_interrupt_available"].as_bool(), Some(true));
        assert_eq!(compact["error_count"].as_u64(), Some(5));
        assert!(compact["errors"][0]["validation_rule_id"].is_string());
        assert!(compact["errors"][0]["file"].is_string());
        assert!(compact["errors"][0]["source_span"].is_object());
        assert!(compact["errors"][0]["recommended_fix"].is_string());
        assert!(compact["errors"][0]["suggested_next_steps"].is_array());
        assert!(compact["claimability"].is_object());
        assert!(compact["lifecycle"].is_object());
        assert!(compact["stale_unsafe_blockers"].is_array());
        assert!(compact["expansion_handles"].is_array());
    }

    #[test]
    fn large_interrupts_truncated_with_omitted_count() {
        let compact = hard_interrupt_packet_with_error_count(6).compact_agent_json(2);
        assert_eq!(compact["errors"].as_array().unwrap().len(), 2);
        assert_eq!(compact["error_count"].as_u64(), Some(6));
        assert!(compact["omitted_count"].as_u64().unwrap_or_default() >= 4);
        assert_eq!(
            compact["aggregate_counts"]["by_validation_rule_id"]["mvp3_3.calls.dangling_target"]
                .as_u64(),
            Some(6)
        );
        assert_eq!(
            compact["aggregate_counts"]["by_relation_kind"]["CALLS"].as_u64(),
            Some(6)
        );
        assert!(compact["aggregate_counts"]["by_file"].is_object());
    }

    #[test]
    fn expansion_handles_present() {
        let compact = hard_interrupt_packet_with_error_count(4).compact_agent_json(1);
        let handles = compact["expansion_handles"]
            .as_array()
            .expect("expansion handles");
        assert!(!handles.is_empty());
        assert!(handles.iter().any(|handle| {
            handle["handle"]
                .as_str()
                .map(|value| value.starts_with("hard_interrupt:error:"))
                .unwrap_or(false)
        }));
        assert!(handles
            .iter()
            .any(|handle| { handle["handle"].as_str() == Some("hard_interrupt_packet:full") }));
    }

    #[test]
    fn default_output_no_full_graph_dump() {
        let mut interrupt = hard_interrupt_packet_with_error_count(2);
        interrupt.source_validation_packet_summary = Some(json!({
            "packet_kind": "graph_validation_packet",
            "status": "blocking_graph_error",
            "blocking_error_count": 2,
            "graph_delta": {
                "nodes": ["full"],
                "edges": ["full"]
            },
            "full_graph_dump": true
        }));
        let compact = interrupt.compact_agent_json(1);
        assert!(compact.get("graph_delta").is_none());
        assert!(compact["source_validation_packet_summary"]
            .get("graph_delta")
            .is_none());
        let serialized = serde_json::to_string(&compact).expect("compact serializes");
        assert!(!serialized.contains("full_graph_dump"));
        assert!(!serialized.contains("\"nodes\""));
        assert!(!serialized.contains("\"edges\""));
    }

    #[test]
    fn default_output_no_full_lifecycle_dump() {
        let mut interrupt = hard_interrupt_packet_with_error_count(2);
        interrupt.lifecycle = json!({
            "claimable": true,
            "current": true,
            "passport_status": "ok",
            "exact_db_path_checked": "C:/repo/.codegraph/codegraph.db",
            "sidecar_manifest": {"path": "C:/repo/.codegraph/sidecar.json"},
            "blockers": ["stale", "dirty"]
        });
        let compact = interrupt.compact_agent_json(1);
        let lifecycle = compact["lifecycle"].as_object().expect("lifecycle object");
        assert_eq!(
            lifecycle.get("claimable").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            lifecycle.get("current").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            lifecycle.get("blocker_count").and_then(Value::as_u64),
            Some(2)
        );
        assert!(!lifecycle.contains_key("exact_db_path_checked"));
        assert!(!lifecycle.contains_key("sidecar_manifest"));
        let serialized = serde_json::to_string(&compact).expect("compact serializes");
        assert!(!serialized.contains(".codegraph"));
    }

    #[test]
    fn max_output_bytes_respected_if_supported() {
        let compact = hard_interrupt_packet_with_error_count(20).compact_agent_json(3);
        let serialized = serde_json::to_string(&compact).expect("compact serializes");
        assert!(
            serialized.len() < 16 * 1024,
            "compact hard interrupt exceeded smoke budget: {} bytes",
            serialized.len()
        );
        assert_eq!(compact["error_count"].as_u64(), Some(20));
        assert!(compact["omitted_count"].as_u64().unwrap_or_default() >= 17);
    }

    #[test]
    fn compact_packet_json_schema_valid() {
        let schema = read_agent_json_schema_for_test("hard_interrupt_agent_json.schema.json");
        let compact = hard_interrupt_packet_with_error_count(3).compact_agent_json(1);
        for field in schema["required"].as_array().expect("required fields") {
            let field = field.as_str().expect("required field string");
            assert!(
                compact.get(field).is_some(),
                "compact packet missing {field}"
            );
        }
        let error_required = schema["$defs"]["hard_interrupt_error"]["required"]
            .as_array()
            .expect("error required fields");
        for field in error_required {
            let field = field.as_str().expect("error required field string");
            assert!(
                compact["errors"][0].get(field).is_some(),
                "compact error missing schema field {field}"
            );
        }
    }

    #[test]
    fn explain_packet_json_schema_valid() {
        let schema = read_agent_json_schema_for_test("hard_interrupt_agent_json.schema.json");
        let full = serde_json::to_value(hard_interrupt_packet_with_error_count(3))
            .expect("full packet serializes");
        for field in schema["required"].as_array().expect("required fields") {
            let field = field.as_str().expect("required field string");
            assert!(full.get(field).is_some(), "full packet missing {field}");
        }
        let error_required = schema["$defs"]["hard_interrupt_error"]["required"]
            .as_array()
            .expect("error required fields");
        for field in error_required {
            let field = field.as_str().expect("error required field string");
            assert!(
                full["errors"][0].get(field).is_some(),
                "full error missing schema field {field}"
            );
        }
        assert!(full["errors"][0]["evidence_items"].is_array());
        assert!(full["errors"][0]["eligibility"].is_object());
    }

    #[test]
    fn no_hard_interrupt_proof_fields_dropped() {
        let compact = hard_interrupt_packet_with_error_count(2).compact_agent_json(1);
        let error = &compact["errors"][0];
        assert!(error["exact_proof_reason"].is_string());
        assert_eq!(error["reverified_graph_source_proof"].as_bool(), Some(true));
        assert!(error["claimability"].is_object());
        assert!(error["lifecycle"].is_object());
        assert!(error["source_span"].is_object());
        assert!(error["recommended_fix"].is_string());
        assert!(error["suggested_next_steps"].is_array());
    }

    #[test]
    fn hard_interrupt_available_false_when_no_blocking_errors() {
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {}}),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );

        assert!(packet.blocking_errors.is_empty());
        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
    }

    #[test]
    fn hard_interrupt_available_true_when_interrupt_packet_present() {
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {}}),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        )
        .with_hard_interrupt_packet(hard_interrupt_packet_example());

        assert!(packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_some());
    }

    #[test]
    fn only_block_findings_interrupt() {
        let rule = exact_calls_rule();
        let block = block_finding_for_rule(&rule, "finding://interrupt/block");
        let packet = claimable_interrupt_packet_for(rule, vec![block]);
        assert!(packet.hard_interrupt_available);
        assert_eq!(
            packet
                .hard_interrupt
                .as_ref()
                .map(|interrupt| interrupt.errors.len()),
            Some(1)
        );
    }

    #[test]
    fn warnings_never_interrupt_by_default() {
        let rule = exact_calls_rule();
        let mut input = exact_graph_source_input();
        input.over_budget = true;
        let finding = classify_validation_finding(&rule, "finding://warning/degraded", input);
        let packet = non_interrupt_packet_for(finding, rule);
        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
    }

    #[test]
    fn warnings_do_not_interrupt() {
        warnings_never_interrupt_by_default();
    }

    #[test]
    fn unknown_never_interrupts() {
        let mut rule = exact_calls_rule();
        rule.supported_relation_status = SupportedRelationStatus::ActivationGated;
        let finding = classify_validation_finding(
            &rule,
            "finding://unknown/activation",
            exact_graph_source_input(),
        );
        let packet = non_interrupt_packet_for(finding, rule);
        assert!(!packet.hard_interrupt_available);
    }

    #[test]
    fn unknowns_do_not_interrupt() {
        unknown_never_interrupts();
    }

    #[test]
    fn unsupported_never_interrupts() {
        let mut rule = exact_calls_rule();
        rule.supported_relation_status = SupportedRelationStatus::Unsupported;
        rule.default_classification_when_unsupported = ValidationClassification::Unsupported;
        let mut input = exact_graph_source_input();
        input.unsupported_relation = true;
        input.relation_supported = false;
        let finding = classify_validation_finding(&rule, "finding://unsupported", input);
        let packet = non_interrupt_packet_for(finding, rule);
        assert!(!packet.hard_interrupt_available);
    }

    #[test]
    fn unsupported_do_not_interrupt() {
        unsupported_never_interrupts();
    }

    #[test]
    fn degraded_never_interrupts() {
        let rule = exact_calls_rule();
        let mut input = exact_graph_source_input();
        input.over_budget = true;
        let finding = classify_validation_finding(&rule, "finding://degraded", input);
        let packet = non_interrupt_packet_for(finding, rule);
        assert!(!packet.hard_interrupt_available);
    }

    #[test]
    fn degraded_do_not_interrupt() {
        degraded_never_interrupts();
    }

    #[test]
    fn diagnostic_never_interrupts() {
        let rule = exact_calls_rule();
        let mut input = exact_graph_source_input();
        input.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::Candidate,
            "candidate://diagnostic",
            "candidate evidence is not graph proof",
        )];
        let finding = classify_validation_finding(&rule, "finding://diagnostic", input);
        let packet = non_interrupt_packet_for(finding, rule);
        assert!(!packet.hard_interrupt_available);
    }

    #[test]
    fn diagnostic_do_not_interrupt() {
        diagnostic_never_interrupts();
    }

    #[test]
    fn interrupt_requires_reverified_graph_source_proof() {
        let rule = exact_calls_rule();
        let mut finding = block_finding_for_rule(&rule, "finding://block/not-reverified");
        finding.reverified_graph_source_proof = false;
        let eligibility = interrupt_eligibility_for_finding(
            &finding,
            Some(&rule),
            &serde_json::json!({"claimable": true, "current": true, "blockers": []}),
        );
        assert!(!eligibility.eligible);
        assert!(eligibility
            .disqualifiers
            .contains(&"finding_not_reverified_graph_source_or_integrity_proof".to_string()));
    }

    #[test]
    fn unverified_block_finding_does_not_interrupt() {
        let rule = exact_calls_rule();
        let mut finding = block_finding_for_rule(&rule, "finding://block/unverified");
        finding.reverified_graph_source_proof = false;
        let packet = claimable_interrupt_packet_for(rule, vec![finding]);

        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
    }

    #[test]
    fn interrupt_requires_claimable_lifecycle() {
        let rule = exact_calls_rule();
        let mut finding = block_finding_for_rule(&rule, "finding://block/stale");
        finding.lifecycle = ValidationLifecycleState::stale_non_claimable("repo_head_mismatch");
        let eligibility = interrupt_eligibility_for_finding(
            &finding,
            Some(&rule),
            &serde_json::json!({"claimable": true, "current": true, "blockers": []}),
        );
        assert!(!eligibility.eligible);
        assert!(!eligibility.lifecycle_ok);
    }

    #[test]
    fn unsafe_lifecycle_rejects_relation_interrupt() {
        let rule = exact_calls_rule();
        let mut finding = block_finding_for_rule(&rule, "finding://block/unsafe-lifecycle");
        finding.lifecycle = ValidationLifecycleState {
            claimable: false,
            current: false,
            stale: false,
            foreign: true,
            schema_mismatched: false,
            dirty: false,
            partial: false,
            non_claimable_reason: Some("foreign_db".to_string()),
        };
        let packet = claimable_interrupt_packet_for(rule, vec![finding]);
        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
    }

    #[test]
    fn unsafe_db_states_do_not_interrupt() {
        for (label, lifecycle) in [
            (
                "stale_db",
                ValidationLifecycleState::stale_non_claimable("repo_head_mismatch"),
            ),
            (
                "foreign_db",
                ValidationLifecycleState {
                    claimable: false,
                    current: false,
                    stale: false,
                    foreign: true,
                    schema_mismatched: false,
                    dirty: false,
                    partial: false,
                    non_claimable_reason: Some("foreign_db".to_string()),
                },
            ),
            (
                "schema_mismatch",
                ValidationLifecycleState {
                    claimable: false,
                    current: false,
                    stale: false,
                    foreign: false,
                    schema_mismatched: true,
                    dirty: false,
                    partial: false,
                    non_claimable_reason: Some("schema_mismatch".to_string()),
                },
            ),
            (
                "dirty_publish_state",
                ValidationLifecycleState {
                    claimable: false,
                    current: false,
                    stale: false,
                    foreign: false,
                    schema_mismatched: false,
                    dirty: true,
                    partial: false,
                    non_claimable_reason: Some("dirty_or_publishing".to_string()),
                },
            ),
            (
                "partial_db",
                ValidationLifecycleState {
                    claimable: false,
                    current: false,
                    stale: false,
                    foreign: false,
                    schema_mismatched: false,
                    dirty: false,
                    partial: true,
                    non_claimable_reason: Some("partial_db".to_string()),
                },
            ),
        ] {
            let rule = exact_calls_rule();
            let mut finding = block_finding_for_rule(&rule, &format!("finding://block/{label}"));
            finding.lifecycle = lifecycle;
            let packet = claimable_interrupt_packet_for(rule, vec![finding]);

            assert!(
                !packet.hard_interrupt_available,
                "{label} unexpectedly created a hard interrupt: {packet:?}"
            );
            assert!(
                packet.hard_interrupt.is_none(),
                "{label} unexpectedly emitted a hard interrupt packet: {packet:?}"
            );
        }
    }

    #[test]
    fn text_candidate_vector_source_navigation_never_interrupt() {
        let rule = exact_calls_rule();
        for evidence_kind in [
            ValidationEvidenceKind::TextEvidence,
            ValidationEvidenceKind::Candidate,
            ValidationEvidenceKind::Vector,
            ValidationEvidenceKind::SourceNavigation,
        ] {
            let mut finding =
                block_finding_for_rule(&rule, &format!("finding://block/{evidence_kind:?}"));
            finding.evidence_items = vec![ValidationEvidenceItem::non_graph(
                evidence_kind,
                format!("evidence://{evidence_kind:?}"),
                "non-graph evidence cannot interrupt",
            )];
            let eligibility = interrupt_eligibility_for_finding(
                &finding,
                Some(&rule),
                &serde_json::json!({"claimable": true, "current": true, "blockers": []}),
            );
            assert!(
                !eligibility.eligible,
                "{evidence_kind:?} unexpectedly interrupt eligible"
            );
            assert!(!eligibility.evidence_kind_ok);
        }
    }

    #[test]
    fn text_candidate_vector_source_navigation_do_not_interrupt() {
        text_candidate_vector_source_navigation_never_interrupt();
    }

    #[test]
    fn stale_sidecar_never_interrupts() {
        let rule = exact_calls_rule();
        let mut finding = block_finding_for_rule(&rule, "finding://block/stale-sidecar");
        finding.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::PathEvidence,
            "sidecar://stale-path-evidence",
            "stale sidecar freshness is not graph/source proof",
        )];
        let eligibility = interrupt_eligibility_for_finding(
            &finding,
            Some(&rule),
            &serde_json::json!({"claimable": true, "current": true, "blockers": []}),
        );
        assert!(!eligibility.eligible);
        assert!(!eligibility.evidence_kind_ok);
    }

    #[test]
    fn stale_sidecar_does_not_interrupt() {
        stale_sidecar_never_interrupts();
    }

    #[test]
    fn over_budget_closure_never_interrupts() {
        let rule = exact_calls_rule();
        let mut input = exact_graph_source_input();
        input.over_budget = true;
        let finding = classify_validation_finding(&rule, "finding://over-budget-closure", input);
        let packet = non_interrupt_packet_for(finding, rule);
        assert!(!packet.hard_interrupt_available);
    }

    #[test]
    fn degraded_closure_does_not_interrupt() {
        over_budget_closure_never_interrupts();
    }

    #[test]
    fn ambiguous_rename_never_interrupts() {
        let mut rule = exact_calls_rule();
        rule.supported_relation_status = SupportedRelationStatus::ActivationGated;
        let mut finding = classify_validation_finding(
            &rule,
            "finding://ambiguous-rename",
            exact_graph_source_input(),
        );
        finding
            .unknowns
            .push("ambiguous rename cannot be graph/source proof".to_string());
        let packet = non_interrupt_packet_for(finding, rule);
        assert!(!packet.hard_interrupt_available);
    }

    #[test]
    fn ambiguous_rename_does_not_interrupt() {
        ambiguous_rename_never_interrupts();
    }

    #[test]
    fn non_blocking_findings_do_not_interrupt() {
        let exact_rule = exact_calls_rule();
        let mut warning_input = exact_graph_source_input();
        warning_input.over_budget = true;
        let warning =
            classify_validation_finding(&exact_rule, "finding://matrix/warning", warning_input);

        let mut unknown_rule = exact_calls_rule();
        unknown_rule.supported_relation_status = SupportedRelationStatus::ActivationGated;
        let unknown = classify_validation_finding(
            &unknown_rule,
            "finding://matrix/unknown",
            exact_graph_source_input(),
        );

        let mut unsupported_rule = exact_calls_rule();
        unsupported_rule.supported_relation_status = SupportedRelationStatus::Unsupported;
        unsupported_rule.default_classification_when_unsupported =
            ValidationClassification::Unsupported;
        let mut unsupported_input = exact_graph_source_input();
        unsupported_input.unsupported_relation = true;
        unsupported_input.relation_supported = false;
        let unsupported = classify_validation_finding(
            &unsupported_rule,
            "finding://matrix/unsupported",
            unsupported_input,
        );

        let mut diagnostic_input = exact_graph_source_input();
        diagnostic_input.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::SourceNavigation,
            "source-navigation://matrix",
            "source-navigation evidence is not graph proof",
        )];
        let diagnostic = classify_validation_finding(
            &exact_rule,
            "finding://matrix/diagnostic",
            diagnostic_input,
        );

        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"adversarial_fixture_count": 4}}),
            vec![warning, unknown, unsupported, diagnostic],
            vec![exact_rule, unknown_rule, unsupported_rule],
            Vec::new(),
            serde_json::json!({"claimable": true, "current": true, "blockers": []}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        )
        .with_eligible_hard_interrupts("test-generated-at");

        assert_eq!(packet.blocking_errors.len(), 0);
        assert_eq!(packet.warnings.len(), 1);
        assert_eq!(packet.unknowns.len(), 2);
        assert_eq!(packet.diagnostics.len(), 1);
        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
    }

    #[test]
    fn mixed_block_warning_interrupt_contains_only_block_errors() {
        let block_rule = exact_calls_rule();
        let warning_rule = exact_imports_rule();
        let block = block_finding_for_rule(&block_rule, "finding://mixed/block");
        let mut warning_input = exact_graph_source_input();
        warning_input.over_budget = true;
        let warning =
            classify_validation_finding(&warning_rule, "finding://mixed/warning", warning_input);
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {"mixed_packet_cases": 1}}),
            vec![block, warning],
            vec![block_rule.clone(), warning_rule],
            Vec::new(),
            serde_json::json!({"claimable": true, "current": true, "blockers": []}),
            serde_json::json!({"graph_relation_proof": {"changed": true}}),
            serde_json::json!({"claimable": true, "current": true}),
        )
        .with_eligible_hard_interrupts("test-generated-at");

        assert!(packet.hard_interrupt_available);
        assert_eq!(packet.blocking_errors.len(), 1);
        assert_eq!(packet.warnings.len(), 1);
        let interrupt = packet.hard_interrupt.as_ref().expect("hard interrupt");
        assert_eq!(interrupt.errors.len(), 1);
        assert_eq!(
            interrupt.errors[0].classification,
            ValidationClassification::Block
        );
        assert_eq!(
            interrupt.errors[0].validation_rule_id,
            block_rule.validation_rule_id
        );
        assert_eq!(interrupt.summary.warning_count, 1);
    }

    #[test]
    fn block_finding_missing_required_source_span_no_relation_interrupt() {
        let rule = exact_calls_rule();
        let mut finding = block_finding_for_rule(&rule, "finding://missing-source-span/relation");
        finding.source_span = None;
        let packet = claimable_interrupt_packet_for(rule, vec![finding]);

        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
    }

    #[test]
    fn false_hard_interrupt_count_zero() {
        let rule = exact_calls_rule();
        let non_interrupt_findings = [
            ValidationClassification::Warn,
            ValidationClassification::Unknown,
            ValidationClassification::Unsupported,
            ValidationClassification::Degraded,
            ValidationClassification::Diagnostic,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, classification)| {
            let mut finding =
                block_finding_for_rule(&rule, &format!("finding://false-hard/{index}"));
            finding.classification = classification;
            finding.blocking_level = match classification {
                ValidationClassification::Warn => ValidationBlockingLevel::Warning,
                ValidationClassification::Unknown => ValidationBlockingLevel::Unknown,
                ValidationClassification::Unsupported => ValidationBlockingLevel::Unknown,
                ValidationClassification::Degraded => ValidationBlockingLevel::Warning,
                ValidationClassification::Diagnostic => ValidationBlockingLevel::DiagnosticOnly,
                ValidationClassification::Block => ValidationBlockingLevel::Blocking,
            };
            finding
        })
        .collect::<Vec<_>>();

        let false_hard_interrupt_count = non_interrupt_findings
            .into_iter()
            .map(|finding| claimable_interrupt_packet_for(rule.clone(), vec![finding]))
            .filter(|packet| packet.hard_interrupt_available || packet.hard_interrupt.is_some())
            .count();

        assert_eq!(false_hard_interrupt_count, 0);
    }

    #[test]
    fn exact_calls_block_finding_interrupts() {
        let rule = exact_calls_rule();
        let packet = claimable_interrupt_packet_for(
            rule.clone(),
            vec![block_finding_for_rule(&rule, "finding://calls/block")],
        );
        assert!(packet.hard_interrupt_available);
        assert_eq!(
            packet.hard_interrupt.as_ref().unwrap().errors[0].relation_kind,
            Some(RelationKind::Calls)
        );
    }

    #[test]
    fn exact_imports_block_finding_interrupts() {
        let rule = exact_imports_rule();
        let packet = claimable_interrupt_packet_for(
            rule.clone(),
            vec![block_finding_for_rule(&rule, "finding://imports/block")],
        );
        assert!(packet.hard_interrupt_available);
        assert_eq!(
            packet.hard_interrupt.as_ref().unwrap().errors[0].relation_kind,
            Some(RelationKind::Imports)
        );
    }

    #[test]
    fn proof_integrity_block_finding_interrupts() {
        let rule = proof_integrity_provenance_rule();
        let mut input = exact_graph_source_input();
        input.graph_source_relation_reverified = false;
        input.integrity_condition_reverified = true;
        input.provenance_required = true;
        input.provenance_present = false;
        input.evidence_items = vec![ValidationEvidenceItem::graph_integrity(
            "edge://derived-missing-provenance",
            "derived graph fact is missing provenance",
        )];
        let mut finding = classify_validation_finding(&rule, "finding://proof-integrity", input);
        finding.affected_file = Some("src/main.rs".to_string());
        finding.source_span = Some(SourceSpan::new("src/main.rs", 1, 1));
        finding.recommended_fix = Some("Attach provenance edges.".to_string());
        let packet = claimable_interrupt_packet_for(rule, vec![finding]);
        assert!(packet.hard_interrupt_available);
    }

    #[test]
    fn proof_integrity_missing_source_span_block_finding_interrupts() {
        let rule = proof_integrity_source_span_rule();
        let mut input = exact_graph_source_input();
        input.graph_source_relation_reverified = false;
        input.integrity_condition_reverified = true;
        input.source_span_required = true;
        input.source_span_present = false;
        input.evidence_items = vec![ValidationEvidenceItem::graph_integrity(
            "edge://missing-source-span",
            "claimable graph fact is missing a source span",
        )];
        let mut finding = classify_validation_finding(&rule, "finding://proof-source-span", input);
        finding.affected_file = Some("src/main.rs".to_string());
        finding.recommended_fix = Some("Regenerate the fact with a source span.".to_string());
        let packet = claimable_interrupt_packet_for(rule, vec![finding]);
        assert!(packet.hard_interrupt_available);
    }

    #[test]
    fn source_role_block_finding_interrupts() {
        let rule = source_role_boundary_rule();
        let mut finding = block_finding_for_rule(&rule, "finding://source-role/block");
        finding.source_role = Some(EvidenceRole::Test);
        let packet = claimable_interrupt_packet_for(rule, vec![finding]);
        assert!(packet.hard_interrupt_available);
    }

    #[test]
    fn activation_gated_block_finding_interrupts_only_if_mvp3_3_block() {
        let activated_rule = activation_gated_reads_rule();
        let packet = claimable_interrupt_packet_for(
            activated_rule.clone(),
            vec![block_finding_for_rule(
                &activated_rule,
                "finding://reads/block",
            )],
        );
        assert!(packet.hard_interrupt_available);

        let mut gated_rule = activation_gated_reads_rule();
        gated_rule.supported_relation_status = SupportedRelationStatus::ActivationGated;
        let finding = classify_validation_finding(
            &gated_rule,
            "finding://reads/not-activated",
            exact_graph_source_input(),
        );
        let packet = non_interrupt_packet_for(finding, gated_rule);
        assert!(!packet.hard_interrupt_available);
    }

    #[test]
    fn no_dot_codegraph_mutation() {
        let serialized = serde_json::to_string(&claimable_interrupt_packet_for(
            exact_calls_rule(),
            vec![block_finding_for_rule(
                &exact_calls_rule(),
                "finding://no-dot-codegraph",
            )],
        ))
        .expect("serialize interrupt packet fixture");
        assert!(!serialized.contains(".codegraph"));
    }

    #[test]
    fn recommended_fix_templates_defined() {
        let cases = [
            (
                exact_calls_rule(),
                "Restore the missing target or update the call to an existing source-spanned target.",
            ),
            (
                exact_imports_rule(),
                "Restore the exported symbol/module or update the import path/alias to the new target.",
            ),
            (
                proof_integrity_source_span_rule(),
                "Regenerate the graph fact from source so the proof edge has a source span, or downgrade it to diagnostic if it cannot be source-spanned.",
            ),
            (
                proof_integrity_provenance_rule(),
                "Attach derivation provenance to the edge or remove/downgrade the derived edge from claimable proof.",
            ),
            (
                source_role_boundary_rule(),
                "Move the proof path to production evidence, or mark the evidence as test/mock/stub/generated and keep it out of production proof.",
            ),
            (
                lifecycle_integrity_rule(),
                "Refresh or rebuild the production agent-use DB, then rerun validation. Do not use the current unsafe DB state as proof.",
            ),
            (
                activation_gated_reads_rule(),
                "Restore or update the exact referenced target, or downgrade the relation if exact proof no longer exists.",
            ),
        ];

        for (rule, expected_fix) in cases {
            let mut finding = block_finding_for_rule(&rule, "finding://template/fix");
            if matches!(rule.rule_kind, ValidationRuleKind::ProofIntegrity) {
                finding.reverified_graph_source_proof = true;
                finding.evidence_items = vec![ValidationEvidenceItem::graph_integrity(
                    "evidence://integrity",
                    "integrity condition reverified",
                )];
            }
            let error = hard_interrupt_error_for(rule, finding);
            assert_eq!(error.recommended_fix, expected_fix);
        }
    }

    #[test]
    fn suggested_next_steps_actionable() {
        let rule = exact_calls_rule();
        let finding = finding_with_endpoint_names(
            &rule,
            "finding://calls/actionable",
            "loadJwt",
            "jwt_timeout",
        );
        let error = hard_interrupt_error_for(rule, finding);
        assert!(error
            .suggested_next_steps
            .iter()
            .any(|step| step.contains("src/main.rs:1:1-1:12")));
        assert!(error
            .suggested_next_steps
            .iter()
            .any(|step| step.contains("jwt_timeout")));
        assert!(error
            .suggested_next_steps
            .iter()
            .any(|step| step.contains("Restore") || step.contains("update")));
    }

    #[test]
    fn rerun_validation_step_included() {
        let rule = exact_imports_rule();
        let finding = finding_with_endpoint_names(
            &rule,
            "finding://imports/rerun",
            "src::consumer",
            "missingExport",
        );
        let error = hard_interrupt_error_for(rule, finding);
        assert!(error.suggested_next_steps.iter().any(|step| step.contains(
            "codegraph-mcp agent-use watch --repo <repo> --once --changed src/main.rs --json"
        )));
    }

    #[test]
    fn messages_do_not_overclaim() {
        let rule = exact_calls_rule();
        let finding = finding_with_endpoint_names(
            &rule,
            "finding://calls/no-overclaim",
            "callJwt",
            "jwt_timeout",
        );
        let error = hard_interrupt_error_for(rule, finding);
        let message = error.message.to_ascii_lowercase();
        for forbidden in [
            "compiler",
            "tests pass",
            "runtime correctness",
            "security vulnerability",
            "vulnerable",
        ] {
            assert!(
                !message.contains(forbidden),
                "message overclaimed with {forbidden}: {}",
                error.message
            );
        }
    }

    #[test]
    fn non_graph_evidence_not_used_for_fix_proof() {
        let rule = exact_calls_rule();
        let mut finding = block_finding_for_rule(&rule, "finding://non-graph/fix-proof");
        finding.evidence_items = vec![ValidationEvidenceItem::non_graph(
            ValidationEvidenceKind::Vector,
            "vector://candidate",
            "vector candidate is not graph proof",
        )];
        let packet = claimable_interrupt_packet_for(rule, vec![finding]);
        assert!(!packet.hard_interrupt_available);
        assert!(packet.hard_interrupt.is_none());
    }

    #[test]
    fn dangling_call_message_includes_file_span_target() {
        let rule = exact_calls_rule();
        let finding =
            finding_with_endpoint_names(&rule, "finding://calls/message", "callJwt", "jwt_timeout");
        let error = hard_interrupt_error_for(rule, finding);
        assert_eq!(error.template_kind, "dangling_exact_calls");
        assert!(error.message.contains("Exact CALLS relation"));
        assert!(error.message.contains("src/main.rs:1:1-1:12"));
        assert!(error.message.contains("callJwt"));
        assert!(error.message.contains("jwt_timeout"));
        assert!(error.message.contains("graph/source recheck"));
        assert!(error.message.contains(&error.validation_rule_id));
        assert!(error.message.contains(&error.invariant));
        assert!(error.message.contains(&error.exact_proof_reason));
    }

    #[test]
    fn dangling_import_message_includes_file_span_target() {
        let rule = exact_imports_rule();
        let finding = finding_with_endpoint_names(
            &rule,
            "finding://imports/message",
            "src::main",
            "missingExport",
        );
        let error = hard_interrupt_error_for(rule, finding);
        assert_eq!(error.template_kind, "dangling_exact_imports");
        assert!(error.message.contains("Exact IMPORTS relation"));
        assert!(error.message.contains("src/main.rs:1:1-1:12"));
        assert!(error.message.contains("missingExport"));
        assert!(error.message.contains(&error.validation_rule_id));
        assert!(error.message.contains(&error.invariant));
        assert!(error.message.contains(&error.exact_proof_reason));
    }

    #[test]
    fn missing_span_message_distinguishes_integrity_from_runtime_behavior() {
        let rule = proof_integrity_source_span_rule();
        let mut input = exact_graph_source_input();
        input.graph_source_relation_reverified = false;
        input.integrity_condition_reverified = true;
        input.source_span_required = true;
        input.source_span_present = false;
        input.evidence_items = vec![ValidationEvidenceItem::graph_integrity(
            "edge://missing-source-span",
            "claimable graph fact is missing a source span",
        )];
        let mut finding =
            classify_validation_finding(&rule, "finding://missing-span/message", input);
        finding.affected_file = Some("src/main.rs".to_string());
        let error = hard_interrupt_error_for(rule, finding);
        assert_eq!(error.template_kind, "missing_source_span");
        assert!(error.message.contains("Graph-integrity rule"));
        assert!(error.message.contains("without a current source span"));
        assert!(!error.message.to_ascii_lowercase().contains("runtime"));
    }

    #[test]
    fn derived_missing_provenance_message_actionable() {
        let rule = proof_integrity_provenance_rule();
        let mut input = exact_graph_source_input();
        input.graph_source_relation_reverified = false;
        input.integrity_condition_reverified = true;
        input.provenance_required = true;
        input.provenance_present = false;
        input.evidence_items = vec![ValidationEvidenceItem::graph_integrity(
            "edge://missing-provenance",
            "derived graph fact is missing provenance",
        )];
        let mut finding =
            classify_validation_finding(&rule, "finding://missing-provenance/message", input);
        finding.affected_file = Some("src/main.rs".to_string());
        finding.source_span = Some(SourceSpan::with_columns("src/main.rs", 1, 1, 1, 12));
        let error = hard_interrupt_error_for(rule, finding);
        assert_eq!(error.template_kind, "derived_missing_provenance");
        assert!(error.message.contains("without provenance"));
        assert!(error
            .suggested_next_steps
            .iter()
            .any(|step| step.contains("Attach derivation provenance")));
    }

    #[test]
    fn source_role_leakage_message_actionable() {
        let rule = source_role_boundary_rule();
        let mut finding = finding_with_endpoint_names(
            &rule,
            "finding://source-role/message",
            "productionCaller",
            "testOnlyHelper",
        );
        finding.source_role = Some(EvidenceRole::Test);
        let error = hard_interrupt_error_for(rule, finding);
        assert_eq!(error.template_kind, "source_role_leakage");
        assert!(error.message.contains("test evidence as production proof"));
        assert!(error.recommended_fix.contains("production evidence"));
    }

    #[test]
    fn lifecycle_message_does_not_claim_source_bug() {
        let rule = lifecycle_integrity_rule();
        let mut input = exact_graph_source_input();
        input.graph_source_relation_reverified = false;
        input.integrity_condition_reverified = true;
        input.integrity_issue_present = true;
        input.source_span_required = false;
        input.evidence_items = vec![ValidationEvidenceItem::graph_integrity(
            "lifecycle://passport",
            "DB passport reports interrupted update",
        )];
        let mut finding = classify_validation_finding(&rule, "finding://lifecycle/message", input);
        finding.affected_file = Some("src/main.rs".to_string());
        let error = hard_interrupt_error_for(rule, finding);
        assert_eq!(error.template_kind, "lifecycle_mismatch");
        assert!(error.message.contains("unsafe validation DB state"));
        assert!(!error.message.contains("source bug"));
    }

    #[test]
    fn generic_fallback_message_safe() {
        let rule = generic_blocking_rule();
        let finding = finding_with_endpoint_names(
            &rule,
            "finding://generic/message",
            "sourceSymbol",
            "targetSymbol",
        );
        let error = hard_interrupt_error_for(rule, finding);
        assert_eq!(error.template_kind, "generic");
        assert!(error.message.contains("Validation rule"));
        assert!(!error
            .message
            .to_ascii_lowercase()
            .contains("security vulnerability"));
    }

    #[test]
    fn hard_interrupt_agent_json_schema_parses_and_has_required_fields() {
        let validation_schema =
            read_agent_json_schema_for_test("validation_packet_agent_json.schema.json");
        let hard_interrupt_schema =
            read_agent_json_schema_for_test("hard_interrupt_agent_json.schema.json");

        assert_eq!(
            validation_schema["properties"]["hard_interrupt_available"]["type"].as_str(),
            Some("boolean")
        );
        assert!(validation_schema["properties"]["hard_interrupt"].is_object());

        let required = hard_interrupt_schema["required"]
            .as_array()
            .expect("hard interrupt required fields");
        for field in [
            "schema_version",
            "packet_kind",
            "status",
            "must_fix_before_continuing",
            "hard_interrupt_available",
            "errors",
            "error_count",
            "warnings_summary",
            "unknowns_summary",
            "diagnostics_summary",
            "summary",
            "claimability",
            "lifecycle",
            "proof_ladder_changes",
            "validation_rules_evaluated",
            "validation_rules_skipped",
            "omitted_count",
            "expansion_handles",
        ] {
            assert!(
                required.iter().any(|item| item.as_str() == Some(field)),
                "hard interrupt schema missing required field {field}"
            );
        }
        assert!(
            hard_interrupt_schema["$defs"]["hard_interrupt_error"]["required"]
                .as_array()
                .expect("hard interrupt error required fields")
                .iter()
                .any(|item| item.as_str() == Some("recommended_fix"))
        );
        assert!(
            hard_interrupt_schema["$defs"]["hard_interrupt_error"]["required"]
                .as_array()
                .expect("hard interrupt error required fields")
                .iter()
                .any(|item| item.as_str() == Some("expansion_handle"))
        );
    }

    #[test]
    fn no_full_graph_dump_in_schema_examples() {
        let serialized =
            serde_json::to_string(&hard_interrupt_packet_example()).expect("packet serializes");
        assert!(!serialized.contains("\"nodes\""));
        assert!(!serialized.contains("\"edges\""));
        assert!(!serialized.contains("full_graph_dump"));
    }

    #[test]
    fn no_full_graph_dump_by_default() {
        let rule = exact_calls_rule();
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({
                "summary": {"entity_delta_count": 1},
                "omitted_count": 0,
                "full_graph_dump_default": false
            }),
            vec![exact_finding_with_span(
                &rule,
                "finding://calls/no-full-dump",
            )],
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );
        let compact = packet.compact_agent_json(1);
        assert_ne!(
            compact["graph_delta"].get("nodes"),
            Some(&serde_json::json!("*"))
        );
        assert_eq!(
            compact["graph_delta"]["full_graph_dump_default"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn no_stale_candidate_vector_evidence_as_proof() {
        let rule = exact_calls_rule();
        for evidence_kind in [
            ValidationEvidenceKind::Candidate,
            ValidationEvidenceKind::Vector,
        ] {
            let mut input = exact_graph_source_input();
            input.graph_source_relation_reverified = false;
            input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                evidence_kind,
                format!("evidence://{evidence_kind:?}/stale"),
                "stale sidecar evidence is freshness state, not graph proof",
            )];
            let finding =
                classify_validation_finding(&rule, format!("finding://{evidence_kind:?}"), input);
            assert_eq!(finding.classification, ValidationClassification::Diagnostic);
            assert!(!finding.reverified_graph_source_proof);
            assert_eq!(
                finding.affected_evidence[0]["graph_proof"].as_bool(),
                Some(false)
            );
        }
    }

    #[test]
    fn validation_contract_does_not_reference_repo_local_codegraph_path() {
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({}),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true}),
        );

        let serialized = serde_json::to_string(&packet).expect("packet serializes");
        assert!(!serialized.contains(".codegraph"));
    }
}
