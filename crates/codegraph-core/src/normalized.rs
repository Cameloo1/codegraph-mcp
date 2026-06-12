use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{
    classify_edge_evidence_role, classify_entity_source_role, normalize_repo_relative_path,
    stable_fact_hash, stable_fact_identity_key, Edge, EdgeContext, Entity, EntityKind,
    EvidenceRole, Exactness, FileRecord, Metadata, PathEvidence, RelationKind, SourceSpan,
};

pub const NORMALIZED_FACT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizedFactKind {
    File,
    Entity,
    Edge,
    SourceSpan,
    SourceRole,
    TextEvidence,
    PathEvidence,
    SidecarFreshness,
    UnresolvedReference,
}

impl NormalizedFactKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Entity => "entity",
            Self::Edge => "edge",
            Self::SourceSpan => "source_span",
            Self::SourceRole => "source_role",
            Self::TextEvidence => "text_evidence",
            Self::PathEvidence => "path_evidence",
            Self::SidecarFreshness => "sidecar_freshness",
            Self::UnresolvedReference => "unresolved_reference",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedClaimabilityMetadata {
    pub graph_proof: bool,
    pub claimable: bool,
    pub proof_status: String,
    pub reason: String,
}

impl NormalizedClaimabilityMetadata {
    pub fn graph_source_proof(reason: impl Into<String>) -> Self {
        Self {
            graph_proof: true,
            claimable: true,
            proof_status: "graph_source_proof".to_string(),
            reason: reason.into(),
        }
    }

    pub fn graph_diagnostic(reason: impl Into<String>) -> Self {
        Self {
            graph_proof: false,
            claimable: false,
            proof_status: "diagnostic_only".to_string(),
            reason: reason.into(),
        }
    }

    pub fn non_graph_proof(reason: impl Into<String>) -> Self {
        Self {
            graph_proof: false,
            claimable: false,
            proof_status: "not_graph_proof".to_string(),
            reason: reason.into(),
        }
    }

    pub fn claimable_non_graph(reason: impl Into<String>) -> Self {
        Self {
            graph_proof: false,
            claimable: true,
            proof_status: "claimable_non_graph_fact".to_string(),
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedLifecycleMetadata {
    pub freshness_status: String,
    pub stale_reason: Option<String>,
    pub db_claimable: bool,
    pub schema_version: u32,
    pub extraction_version: String,
}

impl NormalizedLifecycleMetadata {
    pub fn current(extraction_version: impl Into<String>) -> Self {
        Self {
            freshness_status: "current".to_string(),
            stale_reason: None,
            db_claimable: true,
            schema_version: NORMALIZED_FACT_SCHEMA_VERSION,
            extraction_version: extraction_version.into(),
        }
    }

    pub fn diagnostic(
        freshness_status: impl Into<String>,
        stale_reason: Option<String>,
        extraction_version: impl Into<String>,
    ) -> Self {
        Self {
            freshness_status: freshness_status.into(),
            stale_reason,
            db_claimable: false,
            schema_version: NORMALIZED_FACT_SCHEMA_VERSION,
            extraction_version: extraction_version.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedFileFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub content_hash: Option<String>,
    pub language: Option<String>,
    pub source_role: EvidenceRole,
    pub size_bytes: u64,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedFileFact {
    pub fn from_file(file: &FileRecord) -> Self {
        let repo_relative_path = normalize_repo_relative_path(&file.repo_relative_path);
        let file_id = repo_relative_path.clone();
        let source_role = source_role_from_metadata_or_path(&file.metadata, &repo_relative_path);
        let size_key = file.size_bytes.to_string();
        let identity_parts = [repo_relative_path.as_str()];
        let hash_parts = [
            repo_relative_path.as_str(),
            file.file_hash.as_str(),
            file.language.as_deref().unwrap_or(""),
            source_role.as_str(),
            size_key.as_str(),
        ];
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::File.as_str(),
                identity_parts,
            ),
            fact_hash: stable_fact_hash(NormalizedFactKind::File.as_str(), hash_parts),
            fact_kind: NormalizedFactKind::File,
            repo_relative_path,
            file_id,
            content_hash: Some(file.file_hash.clone()).filter(|value| !value.is_empty()),
            language: file.language.clone(),
            source_role,
            size_bytes: file.size_bytes,
            claimability: NormalizedClaimabilityMetadata::claimable_non_graph(
                "file manifest fact is path-scoped but not graph-relation proof",
            ),
            lifecycle: NormalizedLifecycleMetadata::current("normalized_file_fact_v1"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedEntityFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub entity_id: String,
    pub entity_kind: EntityKind,
    pub name: String,
    pub qualified_name: String,
    pub source_span: Option<SourceSpan>,
    pub source_role: EvidenceRole,
    pub content_hash: Option<String>,
    pub file_hash: Option<String>,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedEntityFact {
    pub fn from_entity(entity: &Entity) -> Self {
        let repo_relative_path = normalize_repo_relative_path(&entity.repo_relative_path);
        let file_id = repo_relative_path.clone();
        let source_role = classify_entity_source_role(entity).role;
        let span_key = optional_span_key(entity.source_span.as_ref());
        let identity_values = [
            entity.id.clone(),
            repo_relative_path.clone(),
            entity.kind.to_string(),
            entity.name.clone(),
            entity.qualified_name.clone(),
            span_key.clone(),
            source_role.as_str().to_string(),
        ];
        let hash_values = [
            entity.id.clone(),
            repo_relative_path.clone(),
            entity.kind.to_string(),
            entity.name.clone(),
            entity.qualified_name.clone(),
            span_key.clone(),
            source_role.as_str().to_string(),
            entity.content_hash.clone().unwrap_or_default(),
            entity.file_hash.clone().unwrap_or_default(),
            metadata_digest_input(&entity.metadata),
        ];
        let claimability = if entity.source_span.is_some() {
            NormalizedClaimabilityMetadata::graph_source_proof(
                "entity fact has a source span in the graph store",
            )
        } else {
            NormalizedClaimabilityMetadata::graph_diagnostic(
                "entity fact lacks a source span and is not claimable graph proof",
            )
        };
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::Entity.as_str(),
                identity_values.iter().map(String::as_str),
            ),
            fact_hash: stable_fact_hash(
                NormalizedFactKind::Entity.as_str(),
                hash_values.iter().map(String::as_str),
            ),
            fact_kind: NormalizedFactKind::Entity,
            repo_relative_path,
            file_id,
            entity_id: entity.id.clone(),
            entity_kind: entity.kind,
            name: entity.name.clone(),
            qualified_name: entity.qualified_name.clone(),
            source_span: entity.source_span.clone(),
            source_role,
            content_hash: entity.content_hash.clone(),
            file_hash: entity.file_hash.clone(),
            claimability,
            lifecycle: NormalizedLifecycleMetadata::current("normalized_entity_fact_v1"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedEdgeFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub edge_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation: RelationKind,
    pub exactness: Exactness,
    pub derived: bool,
    pub provenance_edges: Vec<String>,
    pub provenance_status: String,
    pub source_span: SourceSpan,
    pub source_role: EvidenceRole,
    pub edge_class: String,
    pub edge_context: EdgeContext,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedEdgeFact {
    pub fn from_edge(edge: &Edge) -> Self {
        let repo_relative_path = normalize_repo_relative_path(&edge.source_span.repo_relative_path);
        let file_id = repo_relative_path.clone();
        let source_role = classify_edge_evidence_role(edge).role;
        let span_key = span_key(&edge.source_span);
        let provenance_key = edge.provenance_edges.join("|");
        let derived_key = edge.derived.to_string();
        let exactness_key = edge.exactness.to_string();
        let relation_key = edge.relation.to_string();
        let identity_values = [
            edge.head_id.clone(),
            edge.tail_id.clone(),
            relation_key.clone(),
            exactness_key.clone(),
            derived_key.clone(),
            provenance_key.clone(),
            span_key.clone(),
            source_role.as_str().to_string(),
            repo_relative_path.clone(),
        ];
        let hash_values = [
            edge.id.clone(),
            edge.head_id.clone(),
            edge.tail_id.clone(),
            relation_key,
            exactness_key,
            derived_key,
            provenance_key.clone(),
            span_key,
            source_role.as_str().to_string(),
            edge.edge_class.to_string(),
            edge.context.to_string(),
            metadata_digest_input(&edge.metadata),
        ];
        let provenance_status = if edge.derived && edge.provenance_edges.is_empty() {
            "missing_required_provenance".to_string()
        } else if edge.derived {
            "derived_with_provenance".to_string()
        } else {
            "base_fact".to_string()
        };
        let claimability = edge_claimability(edge, &provenance_status);
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::Edge.as_str(),
                identity_values.iter().map(String::as_str),
            ),
            fact_hash: stable_fact_hash(
                NormalizedFactKind::Edge.as_str(),
                hash_values.iter().map(String::as_str),
            ),
            fact_kind: NormalizedFactKind::Edge,
            repo_relative_path,
            file_id,
            edge_id: edge.id.clone(),
            source_entity_id: edge.head_id.clone(),
            target_entity_id: edge.tail_id.clone(),
            relation: edge.relation,
            exactness: edge.exactness,
            derived: edge.derived,
            provenance_edges: edge.provenance_edges.clone(),
            provenance_status,
            source_span: edge.source_span.clone(),
            source_role,
            edge_class: edge.edge_class.to_string(),
            edge_context: edge.context,
            claimability,
            lifecycle: NormalizedLifecycleMetadata::current("normalized_edge_fact_v1"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedSourceSpanFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub source_span_id: String,
    pub associated_fact_key: String,
    pub associated_fact_kind: String,
    pub source_span: SourceSpan,
    pub source_role: EvidenceRole,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedSourceSpanFact {
    pub fn new(
        source_span_id: impl Into<String>,
        associated_fact_key: impl Into<String>,
        associated_fact_kind: impl Into<String>,
        source_span: SourceSpan,
        source_role: EvidenceRole,
    ) -> Self {
        let source_span_id = source_span_id.into();
        let associated_fact_key = associated_fact_key.into();
        let associated_fact_kind = associated_fact_kind.into();
        let repo_relative_path = normalize_repo_relative_path(&source_span.repo_relative_path);
        let file_id = repo_relative_path.clone();
        let span_key = span_key(&source_span);
        let identity_values = [
            repo_relative_path.clone(),
            span_key.clone(),
            associated_fact_key.clone(),
            associated_fact_kind.clone(),
        ];
        let hash_values = [
            source_span_id.clone(),
            repo_relative_path.clone(),
            span_key,
            associated_fact_key.clone(),
            associated_fact_kind.clone(),
            source_role.as_str().to_string(),
        ];
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::SourceSpan.as_str(),
                identity_values.iter().map(String::as_str),
            ),
            fact_hash: stable_fact_hash(
                NormalizedFactKind::SourceSpan.as_str(),
                hash_values.iter().map(String::as_str),
            ),
            fact_kind: NormalizedFactKind::SourceSpan,
            repo_relative_path,
            file_id,
            source_span_id,
            associated_fact_key,
            associated_fact_kind,
            source_span,
            source_role,
            claimability: NormalizedClaimabilityMetadata::graph_source_proof(
                "source span fact is source-bound to an associated graph fact",
            ),
            lifecycle: NormalizedLifecycleMetadata::current("normalized_source_span_fact_v1"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedSourceRoleFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub source_role: EvidenceRole,
    pub reason: String,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedSourceRoleFact {
    pub fn new(
        repo_relative_path: impl AsRef<str>,
        subject_kind: impl Into<String>,
        subject_id: impl Into<String>,
        source_role: EvidenceRole,
        reason: impl Into<String>,
    ) -> Self {
        let repo_relative_path = normalize_repo_relative_path(repo_relative_path);
        let file_id = repo_relative_path.clone();
        let subject_kind = subject_kind.into();
        let subject_id = subject_id.into();
        let reason = reason.into();
        let identity_values = [
            repo_relative_path.clone(),
            subject_kind.clone(),
            subject_id.clone(),
        ];
        let hash_values = [
            repo_relative_path.clone(),
            subject_kind.clone(),
            subject_id.clone(),
            source_role.as_str().to_string(),
            reason.clone(),
        ];
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::SourceRole.as_str(),
                identity_values.iter().map(String::as_str),
            ),
            fact_hash: stable_fact_hash(
                NormalizedFactKind::SourceRole.as_str(),
                hash_values.iter().map(String::as_str),
            ),
            fact_kind: NormalizedFactKind::SourceRole,
            repo_relative_path,
            file_id,
            subject_kind,
            subject_id,
            source_role,
            reason,
            claimability: NormalizedClaimabilityMetadata::claimable_non_graph(
                "source role is classification metadata, not standalone graph-relation proof",
            ),
            lifecycle: NormalizedLifecycleMetadata::current("normalized_source_role_fact_v1"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedTextEvidenceFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub text_evidence_id: String,
    pub text_kind: String,
    pub line: Option<u32>,
    pub title: String,
    pub text_hash: String,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedTextEvidenceFact {
    pub fn new(
        repo_relative_path: impl AsRef<str>,
        text_evidence_id: impl Into<String>,
        text_kind: impl Into<String>,
        line: Option<u32>,
        title: impl Into<String>,
        text: impl AsRef<str>,
    ) -> Self {
        let repo_relative_path = normalize_repo_relative_path(repo_relative_path);
        let file_id = repo_relative_path.clone();
        let text_evidence_id = text_evidence_id.into();
        let text_kind = text_kind.into();
        let title = title.into();
        let line_key = line.map(|value| value.to_string()).unwrap_or_default();
        let text_hash = stable_fact_hash("text_evidence_body", [text.as_ref()]);
        let identity_values = [
            repo_relative_path.clone(),
            text_kind.clone(),
            text_evidence_id.clone(),
            line_key.clone(),
        ];
        let hash_values = [
            repo_relative_path.clone(),
            text_kind.clone(),
            text_evidence_id.clone(),
            line_key,
            title.clone(),
            text_hash.clone(),
        ];
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::TextEvidence.as_str(),
                identity_values.iter().map(String::as_str),
            ),
            fact_hash: stable_fact_hash(
                NormalizedFactKind::TextEvidence.as_str(),
                hash_values.iter().map(String::as_str),
            ),
            fact_kind: NormalizedFactKind::TextEvidence,
            repo_relative_path,
            file_id,
            text_evidence_id,
            text_kind,
            line,
            title,
            text_hash,
            claimability: NormalizedClaimabilityMetadata::non_graph_proof(
                "text evidence freshness is not graph proof",
            ),
            lifecycle: NormalizedLifecycleMetadata::current("normalized_text_evidence_fact_v1"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedPathEvidenceFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub path_evidence_id: String,
    pub source: String,
    pub target: String,
    pub metapath: Vec<RelationKind>,
    pub source_spans: Vec<SourceSpan>,
    pub exactness: Exactness,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedPathEvidenceFact {
    pub fn from_path_evidence(repo_relative_path: impl AsRef<str>, path: &PathEvidence) -> Self {
        let repo_relative_path = normalize_repo_relative_path(repo_relative_path);
        let file_id = repo_relative_path.clone();
        let metapath_key = path
            .metapath
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(">");
        let span_key = path
            .source_spans
            .iter()
            .map(span_key)
            .collect::<Vec<_>>()
            .join("|");
        let identity_values = [repo_relative_path.clone(), path.id.clone()];
        let hash_values = [
            repo_relative_path.clone(),
            path.id.clone(),
            path.source.clone(),
            path.target.clone(),
            metapath_key,
            span_key,
            path.exactness.to_string(),
            path.length.to_string(),
        ];
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::PathEvidence.as_str(),
                identity_values.iter().map(String::as_str),
            ),
            fact_hash: stable_fact_hash(
                NormalizedFactKind::PathEvidence.as_str(),
                hash_values.iter().map(String::as_str),
            ),
            fact_kind: NormalizedFactKind::PathEvidence,
            repo_relative_path,
            file_id,
            path_evidence_id: path.id.clone(),
            source: path.source.clone(),
            target: path.target.clone(),
            metapath: path.metapath.clone(),
            source_spans: path.source_spans.clone(),
            exactness: path.exactness,
            claimability: NormalizedClaimabilityMetadata::non_graph_proof(
                "PathEvidence is cached path evidence and must be graph/source verified before proof use",
            ),
            lifecycle: NormalizedLifecycleMetadata::current("normalized_path_evidence_fact_v1"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedSidecarFreshnessFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub sidecar_layer: String,
    pub source_binding_id: Option<String>,
    pub freshness_status: String,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedSidecarFreshnessFact {
    pub fn new(
        repo_relative_path: impl AsRef<str>,
        sidecar_layer: impl Into<String>,
        source_binding_id: Option<String>,
        freshness_status: impl Into<String>,
        stale_reason: Option<String>,
    ) -> Self {
        let repo_relative_path = normalize_repo_relative_path(repo_relative_path);
        let file_id = repo_relative_path.clone();
        let sidecar_layer = sidecar_layer.into();
        let freshness_status = freshness_status.into();
        let binding = source_binding_id.clone().unwrap_or_default();
        let stale = stale_reason.clone().unwrap_or_default();
        let identity_values = [
            repo_relative_path.clone(),
            sidecar_layer.clone(),
            binding.clone(),
        ];
        let hash_values = [
            repo_relative_path.clone(),
            sidecar_layer.clone(),
            binding,
            freshness_status.clone(),
            stale,
        ];
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::SidecarFreshness.as_str(),
                identity_values.iter().map(String::as_str),
            ),
            fact_hash: stable_fact_hash(
                NormalizedFactKind::SidecarFreshness.as_str(),
                hash_values.iter().map(String::as_str),
            ),
            fact_kind: NormalizedFactKind::SidecarFreshness,
            repo_relative_path,
            file_id,
            sidecar_layer,
            source_binding_id,
            freshness_status: freshness_status.clone(),
            claimability: NormalizedClaimabilityMetadata::non_graph_proof(
                "sidecar freshness is not graph proof",
            ),
            lifecycle: NormalizedLifecycleMetadata::diagnostic(
                freshness_status,
                stale_reason,
                "normalized_sidecar_freshness_fact_v1",
            ),
        }
    }
}

/// A persisted unresolved-reference lane row normalized for snapshot/delta
/// use. Explicitly non-proof: an unresolved reference is the absence of a
/// link, and must never feed proof-ladder rungs above text/candidate evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedUnresolvedReferenceFact {
    pub stable_identity_key: String,
    pub fact_hash: String,
    pub fact_kind: NormalizedFactKind,
    pub repo_relative_path: String,
    pub file_id: String,
    pub reference_id: String,
    pub name: String,
    pub relation: RelationKind,
    pub source_span: SourceSpan,
    pub reference_class: String,
    pub exactness: Exactness,
    pub extractor: String,
    pub claimability: NormalizedClaimabilityMetadata,
    pub lifecycle: NormalizedLifecycleMetadata,
}

impl NormalizedUnresolvedReferenceFact {
    pub fn new(
        reference_id: impl Into<String>,
        name: impl Into<String>,
        relation: RelationKind,
        source_span: SourceSpan,
        reference_class: impl Into<String>,
        exactness: Exactness,
        extractor: impl Into<String>,
    ) -> Self {
        let reference_id = reference_id.into();
        let name = name.into();
        let reference_class = reference_class.into();
        let extractor = extractor.into();
        let repo_relative_path = normalize_repo_relative_path(&source_span.repo_relative_path);
        let file_id = repo_relative_path.clone();
        let span_key = span_key(&source_span);
        // Spec identity key: (path, span, relation, name).
        let identity_values = [
            repo_relative_path.clone(),
            span_key.clone(),
            relation.to_string(),
            name.clone(),
        ];
        let hash_values = [
            repo_relative_path.clone(),
            span_key,
            relation.to_string(),
            name.clone(),
            reference_class.clone(),
            exactness.to_string(),
            extractor.clone(),
        ];
        Self {
            stable_identity_key: stable_fact_identity_key(
                NormalizedFactKind::UnresolvedReference.as_str(),
                identity_values.iter().map(String::as_str),
            ),
            fact_hash: stable_fact_hash(
                NormalizedFactKind::UnresolvedReference.as_str(),
                hash_values.iter().map(String::as_str),
            ),
            fact_kind: NormalizedFactKind::UnresolvedReference,
            repo_relative_path,
            file_id,
            reference_id,
            name,
            relation,
            source_span,
            reference_class,
            exactness,
            extractor,
            claimability: NormalizedClaimabilityMetadata::claimable_non_graph(
                "unresolved reference is absence of a link; claimable as source-text reference only, never graph proof",
            ),
            lifecycle: NormalizedLifecycleMetadata::current(
                "normalized_unresolved_reference_fact_v1",
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NormalizedFactEnvelope {
    pub stable_identity_key: String,
    pub fact_kind: NormalizedFactKind,
    pub fact_hash: String,
}

impl NormalizedFactEnvelope {
    pub fn new(
        stable_identity_key: impl Into<String>,
        fact_kind: NormalizedFactKind,
        fact_hash: impl Into<String>,
    ) -> Self {
        Self {
            stable_identity_key: stable_identity_key.into(),
            fact_kind,
            fact_hash: fact_hash.into(),
        }
    }
}

macro_rules! impl_envelope_from_fact {
    ($fact:ty) => {
        impl From<&$fact> for NormalizedFactEnvelope {
            fn from(fact: &$fact) -> Self {
                Self::new(
                    fact.stable_identity_key.clone(),
                    fact.fact_kind,
                    fact.fact_hash.clone(),
                )
            }
        }
    };
}

impl_envelope_from_fact!(NormalizedFileFact);
impl_envelope_from_fact!(NormalizedEntityFact);
impl_envelope_from_fact!(NormalizedEdgeFact);
impl_envelope_from_fact!(NormalizedSourceSpanFact);
impl_envelope_from_fact!(NormalizedSourceRoleFact);
impl_envelope_from_fact!(NormalizedTextEvidenceFact);
impl_envelope_from_fact!(NormalizedPathEvidenceFact);
impl_envelope_from_fact!(NormalizedSidecarFreshnessFact);
impl_envelope_from_fact!(NormalizedUnresolvedReferenceFact);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedFactChangeSet {
    pub added: Vec<NormalizedFactEnvelope>,
    pub removed: Vec<NormalizedFactEnvelope>,
    pub changed: Vec<NormalizedFactEnvelope>,
    pub unchanged: usize,
}

pub fn classify_normalized_fact_changes(
    old: &[NormalizedFactEnvelope],
    new: &[NormalizedFactEnvelope],
) -> NormalizedFactChangeSet {
    let old_by_key = old
        .iter()
        .map(|fact| (fact.stable_identity_key.clone(), fact))
        .collect::<std::collections::BTreeMap<_, _>>();
    let new_by_key = new
        .iter()
        .map(|fact| (fact.stable_identity_key.clone(), fact))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut changes = NormalizedFactChangeSet::default();

    for (key, new_fact) in &new_by_key {
        match old_by_key.get(key) {
            None => changes.added.push((*new_fact).clone()),
            Some(old_fact) if old_fact.fact_hash != new_fact.fact_hash => {
                changes.changed.push((*new_fact).clone());
            }
            Some(_) => changes.unchanged += 1,
        }
    }
    for (key, old_fact) in &old_by_key {
        if !new_by_key.contains_key(key) {
            changes.removed.push((*old_fact).clone());
        }
    }
    changes
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedFactOmission {
    pub truncated: bool,
    pub omitted_count: usize,
    pub limit: Option<usize>,
    pub reason: Option<String>,
}

fn source_role_from_metadata_or_path(
    metadata: &Metadata,
    repo_relative_path: &str,
) -> EvidenceRole {
    for key in [
        "evidence_role",
        "source_role",
        "path_context",
        "context",
        "execution_context",
        "scope",
    ] {
        if let Some(value) = metadata.get(key).and_then(serde_json::Value::as_str) {
            if let Ok(role) = EvidenceRole::from_str(value) {
                return role;
            }
        }
    }
    if path_looks_test(repo_relative_path) {
        EvidenceRole::Test
    } else {
        EvidenceRole::Production
    }
}

fn path_looks_test(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("/tests/")
        || normalized.contains("/test/")
        || normalized.ends_with(".test.ts")
        || normalized.ends_with(".test.tsx")
        || normalized.ends_with(".test.js")
        || normalized.ends_with(".test.jsx")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".spec.js")
        || normalized.ends_with(".spec.jsx")
}

fn metadata_digest_input(metadata: &Metadata) -> String {
    serde_json::to_string(metadata).unwrap_or_else(|_| "metadata_unserializable".to_string())
}

fn optional_span_key(span: Option<&SourceSpan>) -> String {
    span.map(span_key)
        .unwrap_or_else(|| "missing_source_span".to_string())
}

fn span_key(span: &SourceSpan) -> String {
    format!(
        "{}:{}:{}-{}:{}",
        normalize_repo_relative_path(&span.repo_relative_path),
        span.start_line,
        span.start_column.unwrap_or(0),
        span.end_line,
        span.end_column.unwrap_or(0)
    )
}

fn edge_claimability(edge: &Edge, provenance_status: &str) -> NormalizedClaimabilityMetadata {
    if provenance_status == "missing_required_provenance" {
        return NormalizedClaimabilityMetadata::graph_diagnostic(
            "derived edge lacks provenance and is diagnostic/unknown, not graph proof",
        );
    }
    if edge_exactness_is_proof_grade(edge.exactness) {
        return NormalizedClaimabilityMetadata::graph_source_proof(
            "edge fact has source span and proof-grade exactness",
        );
    }
    NormalizedClaimabilityMetadata::graph_diagnostic(
        "edge exactness is heuristic/inferred/dynamic and not blocking graph proof",
    )
}

fn edge_exactness_is_proof_grade(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
            | Exactness::DerivedFromVerifiedEdges
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EdgeClass, EdgeContext};

    fn entity(path: &str, name: &str, line: u32, role: EvidenceRole) -> Entity {
        let mut metadata = Metadata::new();
        metadata.insert(
            "source_role".to_string(),
            serde_json::Value::String(role.as_str().to_string()),
        );
        Entity {
            id: format!("repo://e/{path}/{name}"),
            kind: EntityKind::Function,
            name: name.to_string(),
            qualified_name: name.to_string(),
            repo_relative_path: path.to_string(),
            source_span: Some(SourceSpan::with_columns(path, line, 1, line, 20)),
            content_hash: None,
            file_hash: None,
            created_from: "test".to_string(),
            confidence: 1.0,
            metadata,
        }
    }

    fn edge(path: &str, exactness: Exactness, derived: bool, provenance: Vec<String>) -> Edge {
        Edge {
            id: format!("edge://{path}/{exactness}"),
            head_id: "repo://e/src/a.ts/run".to_string(),
            relation: RelationKind::Calls,
            tail_id: "repo://e/src/a.ts/helper".to_string(),
            source_span: SourceSpan::with_columns(path, 2, 3, 2, 12),
            repo_commit: None,
            file_hash: None,
            extractor: "test".to_string(),
            confidence: 1.0,
            exactness,
            edge_class: if derived {
                EdgeClass::Derived
            } else {
                EdgeClass::BaseExact
            },
            context: EdgeContext::Production,
            derived,
            provenance_edges: provenance,
            metadata: Metadata::new(),
        }
    }

    #[test]
    fn same_name_symbols_across_files_have_distinct_entity_keys() {
        let left = NormalizedEntityFact::from_entity(&entity(
            "src/a.ts",
            "shared",
            1,
            EvidenceRole::Production,
        ));
        let right = NormalizedEntityFact::from_entity(&entity(
            "src/b.ts",
            "shared",
            1,
            EvidenceRole::Production,
        ));

        assert_ne!(left.stable_identity_key, right.stable_identity_key);
        assert_eq!(left.name, right.name);
    }

    #[test]
    fn duplicate_content_paths_keep_distinct_file_identity() {
        let left = FileRecord {
            repo_relative_path: "src/a.ts".to_string(),
            file_hash: "same-content".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 42,
            indexed_at_unix_ms: Some(1),
            metadata: Metadata::new(),
        };
        let right = FileRecord {
            repo_relative_path: "src/b.ts".to_string(),
            ..left.clone()
        };

        let left = NormalizedFileFact::from_file(&left);
        let right = NormalizedFileFact::from_file(&right);
        assert_ne!(left.stable_identity_key, right.stable_identity_key);
        assert_eq!(left.content_hash, right.content_hash);
    }

    #[test]
    fn entity_identity_includes_source_role_and_span() {
        let prod = NormalizedEntityFact::from_entity(&entity(
            "src/auth.ts",
            "login",
            3,
            EvidenceRole::Production,
        ));
        let test = NormalizedEntityFact::from_entity(&entity(
            "src/auth.ts",
            "login",
            3,
            EvidenceRole::Test,
        ));
        let moved = NormalizedEntityFact::from_entity(&entity(
            "src/auth.ts",
            "login",
            9,
            EvidenceRole::Production,
        ));

        assert_ne!(prod.stable_identity_key, test.stable_identity_key);
        assert_ne!(prod.stable_identity_key, moved.stable_identity_key);
    }

    #[test]
    fn edge_identity_includes_exactness_provenance_and_source_role() {
        let exact = NormalizedEdgeFact::from_edge(&edge(
            "src/auth.ts",
            Exactness::ParserVerified,
            false,
            Vec::new(),
        ));
        let heuristic = NormalizedEdgeFact::from_edge(&edge(
            "src/auth.ts",
            Exactness::StaticHeuristic,
            false,
            Vec::new(),
        ));
        let derived = NormalizedEdgeFact::from_edge(&edge(
            "src/auth.ts",
            Exactness::DerivedFromVerifiedEdges,
            true,
            vec!["edge://base".to_string()],
        ));
        let test_role = NormalizedEdgeFact::from_edge(&edge(
            "tests/auth.test.ts",
            Exactness::ParserVerified,
            false,
            Vec::new(),
        ));

        assert_ne!(exact.stable_identity_key, heuristic.stable_identity_key);
        assert_ne!(exact.stable_identity_key, derived.stable_identity_key);
        assert_ne!(exact.stable_identity_key, test_role.stable_identity_key);
        assert_eq!(test_role.source_role, EvidenceRole::Test);
    }

    #[test]
    fn derived_edge_without_provenance_is_diagnostic() {
        let fact = NormalizedEdgeFact::from_edge(&edge(
            "src/auth.ts",
            Exactness::DerivedFromVerifiedEdges,
            true,
            Vec::new(),
        ));

        assert_eq!(fact.provenance_status, "missing_required_provenance");
        assert!(!fact.claimability.graph_proof);
        assert!(!fact.claimability.claimable);
    }

    #[test]
    fn source_span_identity_changes_when_span_changes() {
        let a = NormalizedSourceSpanFact::new(
            "span-a",
            "fact-a",
            "entity",
            SourceSpan::with_columns("src/auth.ts", 1, 1, 1, 10),
            EvidenceRole::Production,
        );
        let b = NormalizedSourceSpanFact::new(
            "span-a",
            "fact-a",
            "entity",
            SourceSpan::with_columns("src/auth.ts", 2, 1, 2, 10),
            EvidenceRole::Production,
        );

        assert_ne!(a.stable_identity_key, b.stable_identity_key);
    }

    #[test]
    fn text_evidence_freshness_is_non_graph_proof() {
        let fact = NormalizedTextEvidenceFact::new(
            "README.md",
            "README.md",
            "file",
            None,
            "README.md",
            "some docs text",
        );

        assert!(!fact.claimability.graph_proof);
        assert!(!fact.claimability.claimable);
        assert_eq!(fact.claimability.proof_status, "not_graph_proof");
    }

    #[test]
    fn normalized_fact_comparison_classifies_added_removed_changed() {
        let old_a = NormalizedFactEnvelope::new("a", NormalizedFactKind::Entity, "hash-old");
        let new_a = NormalizedFactEnvelope::new("a", NormalizedFactKind::Entity, "hash-new");
        let old_b = NormalizedFactEnvelope::new("b", NormalizedFactKind::Entity, "hash-b");
        let new_c = NormalizedFactEnvelope::new("c", NormalizedFactKind::Entity, "hash-c");

        let changes = classify_normalized_fact_changes(&[old_a, old_b], &[new_a, new_c]);
        assert_eq!(changes.changed.len(), 1);
        assert_eq!(changes.removed.len(), 1);
        assert_eq!(changes.added.len(), 1);
        assert_eq!(changes.unchanged, 0);
    }
}
