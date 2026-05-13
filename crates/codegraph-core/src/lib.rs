//! Core domain model for the deterministic CodeGraph graph.
//!
//! Phase 02 defines serializable graph entities, relation kinds, source spans,
//! stable IDs, provenance-bearing edges, path evidence, derived edges, context
//! packets, and lightweight relation endpoint validation. This crate
//! intentionally contains no database, parser, vector, MCP, UI, or benchmark
//! implementation.

#![forbid(unsafe_code)]

mod ids;
mod kinds;
mod model;
mod validation;

pub use ids::{
    normalize_repo_relative_path, stable_edge_id, stable_entity_id, stable_entity_id_for_kind,
};
pub use kinds::{EdgeClass, EdgeContext, EntityKind, Exactness, ParseEnumError, RelationKind};
pub use model::{
    infer_edge_class, infer_edge_context, normalize_edge_classification, ContextPacket,
    ContextSnippet, DerivedClosureEdge, Edge, Entity, FileRecord, Metadata, PathEvidence,
    RepoIndexState, SourceSpan,
};
pub use validation::{relation_allows, RelationEndpointClass};

#[cfg(test)]
mod tests {
    use std::{fmt::Debug, str::FromStr};

    use super::{
        relation_allows, stable_edge_id, stable_entity_id, stable_entity_id_for_kind,
        ContextPacket, ContextSnippet, DerivedClosureEdge, Edge, EdgeClass, EdgeContext, Entity,
        EntityKind, Exactness, FileRecord, PathEvidence, RelationKind, RepoIndexState, SourceSpan,
    };

    fn ok<T, E: Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("expected Ok(..), got Err({error:?})"),
        }
    }

    #[test]
    fn workspace_smoke() {
        assert_eq!(env!("CARGO_PKG_NAME"), "codegraph-core");
    }

    #[test]
    fn entity_kind_covers_exact_mvp_entity_types() {
        let names = EntityKind::ALL
            .iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "Repository",
                "Package",
                "Module",
                "Directory",
                "File",
                "Class",
                "Interface",
                "Trait",
                "Enum",
                "Function",
                "Method",
                "Constructor",
                "Parameter",
                "LocalVariable",
                "GlobalVariable",
                "Field",
                "Property",
                "Type",
                "GenericType",
                "Expression",
                "Assignment",
                "CallSite",
                "ReturnSite",
                "Import",
                "Export",
                "Route",
                "Endpoint",
                "Middleware",
                "AuthPolicy",
                "Role",
                "Permission",
                "Sanitizer",
                "Validator",
                "Database",
                "Table",
                "Column",
                "Migration",
                "Event",
                "Topic",
                "Queue",
                "Job",
                "Promise",
                "Task",
                "TestFile",
                "TestSuite",
                "TestCase",
                "Fixture",
                "Mock",
                "Stub",
                "Assertion",
                "ConfigKey",
                "EnvVar",
                "Dependency",
                "PathEvidence",
                "DerivedClosureEdge",
            ]
        );
    }

    #[test]
    fn required_relation_names_are_present() {
        let names = RelationKind::ALL
            .iter()
            .map(|relation| relation.as_str())
            .collect::<Vec<_>>();

        for required in [
            "CALLS",
            "READS",
            "WRITES",
            "FLOWS_TO",
            "IMPLEMENTS",
            "TESTS",
            "AUTHORIZES",
            "CHECKS_ROLE",
            "SANITIZES",
            "EXPOSES",
            "INJECTS",
            "INSTANTIATES",
            "EXTENDS",
            "PUBLISHES",
            "EMITS",
            "CONSUMES",
            "LISTENS_TO",
            "SPAWNS",
            "AWAITS",
            "MUTATES",
            "MIGRATES",
            "ALIASED_BY",
            "MOCKS",
            "STUBS",
            "ASSERTS",
        ] {
            assert!(names.contains(&required), "missing {required}");
        }
    }

    #[test]
    fn relation_parsing_accepts_wire_and_human_spellings() {
        assert_eq!(ok(RelationKind::from_str("CALLS")), RelationKind::Calls);
        assert_eq!(ok(RelationKind::from_str("calls")), RelationKind::Calls);
        assert_eq!(
            ok(RelationKind::from_str("argument_0")),
            RelationKind::Argument0
        );
        assert_eq!(
            ok(RelationKind::from_str("control-depends-on")),
            RelationKind::ControlDependsOn
        );
        assert!(RelationKind::from_str("NOT_A_RELATION").is_err());
    }

    #[test]
    fn exactness_serializes_as_mvp_values() {
        let value = ok(serde_json::to_string(&Exactness::LspVerified));
        assert_eq!(value, "\"lsp_verified\"");

        let parsed: Exactness = ok(serde_json::from_str("\"derived_from_verified_edges\""));
        assert_eq!(parsed, Exactness::DerivedFromVerifiedEdges);
    }

    #[test]
    fn stable_id_generation_is_deterministic_and_path_normalized() {
        let id = stable_entity_id(
            ".\\src\\auth.ts",
            "class:AuthService.method:login(signature_hash)",
        );
        assert_eq!(
            id,
            stable_entity_id(
                "src/auth.ts",
                "class:AuthService.method:login(signature_hash)"
            )
        );
        assert!(id.starts_with("repo://e/"));
        assert_eq!(id.len(), "repo://e/".len() + 32);

        let method_id = stable_entity_id_for_kind(
            "src/auth.ts",
            EntityKind::Method,
            "AuthService.login",
            Some("abc123"),
        );
        assert!(method_id.starts_with("repo://e/"));
        assert_ne!(method_id, id);
    }

    #[test]
    fn edge_id_generation_is_stable() {
        let span = SourceSpan::new("src/auth.ts", 82, 91);
        let id = stable_edge_id(
            "AuthService.login",
            RelationKind::Calls,
            "TokenStore.create",
            &span,
        );

        assert_eq!(
            id,
            stable_edge_id(
                "AuthService.login",
                RelationKind::Calls,
                "TokenStore.create",
                &span
            )
        );
        assert!(id.starts_with("edge://"));
        assert_eq!(id.len(), "edge://".len() + 32);
        assert_ne!(
            id,
            stable_edge_id(
                "AuthService.login",
                RelationKind::Calls,
                "TokenStore.update",
                &span
            )
        );
    }

    #[test]
    fn relation_domain_codomain_validation_works() {
        assert!(relation_allows(
            RelationKind::Calls,
            EntityKind::Method,
            EntityKind::Function
        ));
        assert!(!relation_allows(
            RelationKind::Calls,
            EntityKind::Table,
            EntityKind::Function
        ));
        assert!(relation_allows(
            RelationKind::ChecksRole,
            EntityKind::Route,
            EntityKind::Role
        ));
        assert!(relation_allows(
            RelationKind::Migrates,
            EntityKind::Migration,
            EntityKind::Column
        ));
        assert!(relation_allows(
            RelationKind::Tests,
            EntityKind::TestCase,
            EntityKind::Method
        ));
        assert!(!relation_allows(
            RelationKind::Tests,
            EntityKind::Function,
            EntityKind::TestCase
        ));
    }

    #[test]
    fn source_span_display_and_parse_round_trip() {
        let span = SourceSpan::new("src\\auth.ts", 82, 91);

        assert_eq!(span.to_string(), "src/auth.ts:82-91");
        assert_eq!(ok("src/auth.ts:82-91".parse::<SourceSpan>()), span);
    }

    #[test]
    fn serialization_round_trip_for_core_structs() {
        let span = SourceSpan::new("src/auth.ts", 82, 91);
        let entity = Entity {
            id: stable_entity_id("src/auth.ts", "class:AuthService"),
            kind: EntityKind::Class,
            name: "AuthService".to_string(),
            qualified_name: "AuthService".to_string(),
            repo_relative_path: "src/auth.ts".to_string(),
            source_span: Some(span.clone()),
            content_hash: None,
            file_hash: Some("sha256:file".to_string()),
            created_from: "fixture".to_string(),
            confidence: 1.0,
            metadata: Default::default(),
        };
        let entity_json = ok(serde_json::to_string(&entity));
        let entity_back: Entity = ok(serde_json::from_str(&entity_json));
        assert_eq!(entity_back, entity);

        let edge = Edge {
            id: stable_edge_id(
                "AuthService.login",
                RelationKind::Calls,
                "TokenStore.create",
                &span,
            ),
            head_id: "AuthService.login".to_string(),
            relation: RelationKind::Calls,
            tail_id: "TokenStore.create".to_string(),
            source_span: span.clone(),
            repo_commit: Some("abc123".to_string()),
            file_hash: Some("sha256:file".to_string()),
            extractor: "fixture".to_string(),
            confidence: 1.0,
            exactness: Exactness::LspVerified,
            edge_class: EdgeClass::BaseExact,
            context: EdgeContext::Production,
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        };
        let edge_json = ok(serde_json::to_string(&edge));
        let edge_back: Edge = ok(serde_json::from_str(&edge_json));
        assert_eq!(edge_back, edge);
    }

    #[test]
    fn serialization_round_trip_for_packets_and_state() {
        let span = SourceSpan::new("src/auth.ts", 82, 91);
        let path = PathEvidence {
            id: stable_entity_id("src/auth.ts", "path:auth-login-token"),
            summary: Some("login writes token".to_string()),
            source: "AuthService.login".to_string(),
            target: "TokenStore.create".to_string(),
            metapath: vec![RelationKind::Calls],
            edges: vec![(
                "AuthService.login".to_string(),
                RelationKind::Calls,
                "TokenStore.create".to_string(),
            )],
            source_spans: vec![span.clone()],
            exactness: Exactness::ParserVerified,
            length: 1,
            confidence: 0.9,
            metadata: Default::default(),
        };
        let derived = DerivedClosureEdge {
            id: stable_edge_id(
                "AuthService.login",
                RelationKind::Mutates,
                "users_table",
                &span,
            ),
            head_id: "AuthService.login".to_string(),
            relation: RelationKind::Mutates,
            tail_id: "users_table".to_string(),
            provenance_edges: vec!["edge://base".to_string()],
            exactness: Exactness::DerivedFromVerifiedEdges,
            confidence: 0.8,
            metadata: Default::default(),
        };
        let packet = ContextPacket {
            task: "Change login".to_string(),
            mode: "impact".to_string(),
            symbols: vec!["AuthService.login".to_string()],
            verified_paths: vec![path],
            risks: vec!["Token assertions may change.".to_string()],
            recommended_tests: vec!["npm test -- auth.spec.ts".to_string()],
            snippets: vec![ContextSnippet {
                file: "src/auth.ts".to_string(),
                lines: "82-91".to_string(),
                text: "login(user)".to_string(),
                reason: "login path".to_string(),
            }],
            metadata: Default::default(),
        };
        let file = FileRecord {
            repo_relative_path: "src/auth.ts".to_string(),
            file_hash: "sha256:file".to_string(),
            language: Some("typescript".to_string()),
            size_bytes: 1024,
            indexed_at_unix_ms: Some(1),
            metadata: Default::default(),
        };
        let state = RepoIndexState {
            repo_id: "repo://fixture".to_string(),
            repo_root: ".".to_string(),
            repo_commit: Some("abc123".to_string()),
            schema_version: 1,
            indexed_at_unix_ms: Some(1),
            files_indexed: 1,
            entity_count: 2,
            edge_count: 1,
            metadata: Default::default(),
        };

        for value in [
            ok(serde_json::to_value(&derived)),
            ok(serde_json::to_value(&packet)),
            ok(serde_json::to_value(&file)),
            ok(serde_json::to_value(&state)),
        ] {
            let serialized = ok(serde_json::to_string(&value));
            let reparsed: serde_json::Value = ok(serde_json::from_str(&serialized));
            assert_eq!(reparsed, value);
        }
    }
}
