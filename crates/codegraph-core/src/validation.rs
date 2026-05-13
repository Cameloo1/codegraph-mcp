use crate::{EntityKind, RelationKind};

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
