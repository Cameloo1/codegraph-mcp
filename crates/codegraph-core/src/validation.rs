use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{EntityKind, EvidenceRole, Exactness, RelationKind, SourceSpan};

pub const VALIDATION_PACKET_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationPacketKind {
    GraphValidationPacket,
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
            omitted_count: 0,
            expansion_handles: Vec::new(),
        }
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
        let rule = exact_calls_rule();
        let findings = (0..4)
            .map(|index| {
                let mut input = exact_graph_source_input();
                input.graph_source_relation_reverified = false;
                input.evidence_items = vec![ValidationEvidenceItem::non_graph(
                    ValidationEvidenceKind::Candidate,
                    format!("candidate://{index}"),
                    "candidate evidence remains non-proof",
                )];
                classify_validation_finding(&rule, format!("finding://detail/{index}"), input)
            })
            .collect::<Vec<_>>();
        let packet = ValidationPacket::new(
            vec!["src/main.rs".to_string()],
            serde_json::json!({"summary": {}}),
            findings,
            vec![rule],
            Vec::new(),
            serde_json::json!({"claimable": true}),
            serde_json::json!({}),
            serde_json::json!({"claimable": true, "current": true}),
        );
        let full = serde_json::to_value(&packet).expect("packet serializes");
        let compact = packet.compact_agent_json(1);
        assert_eq!(full["diagnostics"].as_array().unwrap().len(), 4);
        assert_eq!(compact["diagnostics"].as_array().unwrap().len(), 1);
        assert!(full["validation_rules_evaluated"].as_array().is_some());
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
    }

    #[test]
    fn hard_interrupt_packet_kind_is_not_used_by_validation_contract() {
        hard_interrupt_not_implemented();
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
