use std::{error::Error, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError {
    type_name: &'static str,
    value: String,
}

impl ParseEnumError {
    fn new(type_name: &'static str, value: &str) -> Self {
        Self {
            type_name,
            value: value.to_string(),
        }
    }
}

impl fmt::Display for ParseEnumError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown {} value: {}",
            self.type_name, self.value
        )
    }
}

impl Error for ParseEnumError {}

macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($variant:ident => $wire:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ParseEnumError;

            fn from_str(raw: &str) -> Result<Self, Self::Err> {
                let wanted = normalize_enum_token(raw);
                Self::ALL
                    .iter()
                    .copied()
                    .find(|value| normalize_enum_token(value.as_str()) == wanted)
                    .ok_or_else(|| ParseEnumError::new(stringify!($name), raw))
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

fn normalize_enum_token(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_uppercase())
        .collect()
}

string_enum! {
    /// MVP entity kinds from `MVP.md` section 6.
    pub enum EntityKind {
        Repository => "Repository",
        Package => "Package",
        Module => "Module",
        Directory => "Directory",
        File => "File",
        Class => "Class",
        Interface => "Interface",
        Trait => "Trait",
        Enum => "Enum",
        Function => "Function",
        Method => "Method",
        Constructor => "Constructor",
        Parameter => "Parameter",
        LocalVariable => "LocalVariable",
        GlobalVariable => "GlobalVariable",
        Field => "Field",
        Property => "Property",
        Type => "Type",
        GenericType => "GenericType",
        Expression => "Expression",
        Assignment => "Assignment",
        CallSite => "CallSite",
        ReturnSite => "ReturnSite",
        Import => "Import",
        Export => "Export",
        Route => "Route",
        Endpoint => "Endpoint",
        Middleware => "Middleware",
        AuthPolicy => "AuthPolicy",
        Role => "Role",
        Permission => "Permission",
        Sanitizer => "Sanitizer",
        Validator => "Validator",
        Database => "Database",
        Table => "Table",
        Column => "Column",
        Migration => "Migration",
        Event => "Event",
        Topic => "Topic",
        Queue => "Queue",
        Job => "Job",
        Promise => "Promise",
        Task => "Task",
        TestFile => "TestFile",
        TestSuite => "TestSuite",
        TestCase => "TestCase",
        Fixture => "Fixture",
        Mock => "Mock",
        Stub => "Stub",
        Assertion => "Assertion",
        ConfigKey => "ConfigKey",
        EnvVar => "EnvVar",
        Dependency => "Dependency",
        PathEvidence => "PathEvidence",
        DerivedClosureEdge => "DerivedClosureEdge",
    }
}

impl EntityKind {
    pub const fn id_prefix(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Package => "package",
            Self::Module => "module",
            Self::Directory => "directory",
            Self::File => "file",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Enum => "enum",
            Self::Function => "function",
            Self::Method => "method",
            Self::Constructor => "constructor",
            Self::Parameter => "parameter",
            Self::LocalVariable => "local_variable",
            Self::GlobalVariable => "global_variable",
            Self::Field => "field",
            Self::Property => "property",
            Self::Type => "type",
            Self::GenericType => "generic_type",
            Self::Expression => "expression",
            Self::Assignment => "assignment",
            Self::CallSite => "callsite",
            Self::ReturnSite => "returnsite",
            Self::Import => "import",
            Self::Export => "export",
            Self::Route => "route",
            Self::Endpoint => "endpoint",
            Self::Middleware => "middleware",
            Self::AuthPolicy => "auth_policy",
            Self::Role => "role",
            Self::Permission => "permission",
            Self::Sanitizer => "sanitizer",
            Self::Validator => "validator",
            Self::Database => "database",
            Self::Table => "table",
            Self::Column => "column",
            Self::Migration => "migration",
            Self::Event => "event",
            Self::Topic => "topic",
            Self::Queue => "queue",
            Self::Job => "job",
            Self::Promise => "promise",
            Self::Task => "task",
            Self::TestFile => "test_file",
            Self::TestSuite => "test_suite",
            Self::TestCase => "test_case",
            Self::Fixture => "fixture",
            Self::Mock => "mock",
            Self::Stub => "stub",
            Self::Assertion => "assertion",
            Self::ConfigKey => "config_key",
            Self::EnvVar => "env_var",
            Self::Dependency => "dependency",
            Self::PathEvidence => "path_evidence",
            Self::DerivedClosureEdge => "derived_closure_edge",
        }
    }
}

string_enum! {
    /// MVP relation kinds from `MVP.md` section 7.
    pub enum RelationKind {
        Contains => "CONTAINS",
        DefinedIn => "DEFINED_IN",
        Defines => "DEFINES",
        Declares => "DECLARES",
        Exports => "EXPORTS",
        Imports => "IMPORTS",
        Reexports => "REEXPORTS",
        BelongsTo => "BELONGS_TO",
        Configures => "CONFIGURES",
        TypeOf => "TYPE_OF",
        Returns => "RETURNS",
        Implements => "IMPLEMENTS",
        Extends => "EXTENDS",
        Overrides => "OVERRIDES",
        Instantiates => "INSTANTIATES",
        Injects => "INJECTS",
        AliasedBy => "ALIASED_BY",
        AliasOf => "ALIAS_OF",
        Calls => "CALLS",
        CalledBy => "CALLED_BY",
        Callee => "CALLEE",
        Argument0 => "ARGUMENT_0",
        Argument1 => "ARGUMENT_1",
        ArgumentN => "ARGUMENT_N",
        ReturnsTo => "RETURNS_TO",
        Spawns => "SPAWNS",
        Awaits => "AWAITS",
        ListensTo => "LISTENS_TO",
        Reads => "READS",
        Writes => "WRITES",
        Mutates => "MUTATES",
        MutatedBy => "MUTATED_BY",
        FlowsTo => "FLOWS_TO",
        ReachingDef => "REACHING_DEF",
        AssignedFrom => "ASSIGNED_FROM",
        ControlDependsOn => "CONTROL_DEPENDS_ON",
        DataDependsOn => "DATA_DEPENDS_ON",
        Authorizes => "AUTHORIZES",
        ChecksRole => "CHECKS_ROLE",
        ChecksPermission => "CHECKS_PERMISSION",
        Sanitizes => "SANITIZES",
        Validates => "VALIDATES",
        Exposes => "EXPOSES",
        TrustBoundary => "TRUST_BOUNDARY",
        SourceOfTaint => "SOURCE_OF_TAINT",
        SinksTo => "SINKS_TO",
        Publishes => "PUBLISHES",
        Emits => "EMITS",
        Consumes => "CONSUMES",
        SubscribesTo => "SUBSCRIBES_TO",
        Handles => "HANDLES",
        Migrates => "MIGRATES",
        ReadsTable => "READS_TABLE",
        WritesTable => "WRITES_TABLE",
        AltersColumn => "ALTERS_COLUMN",
        DependsOnSchema => "DEPENDS_ON_SCHEMA",
        Tests => "TESTS",
        Asserts => "ASSERTS",
        Mocks => "MOCKS",
        Stubs => "STUBS",
        Covers => "COVERS",
        FixturesFor => "FIXTURES_FOR",
        MayMutate => "MAY_MUTATE",
        MayRead => "MAY_READ",
        ApiReaches => "API_REACHES",
        AsyncReaches => "ASYNC_REACHES",
        SchemaImpact => "SCHEMA_IMPACT",
    }
}

string_enum! {
    /// MVP edge exactness labels from `MVP.md` section 7.
    pub enum Exactness {
        Exact => "exact",
        CompilerVerified => "compiler_verified",
        LspVerified => "lsp_verified",
        ParserVerified => "parser_verified",
        StaticHeuristic => "static_heuristic",
        DynamicTrace => "dynamic_trace",
        Inferred => "inferred",
        DerivedFromVerifiedEdges => "derived_from_verified_edges",
    }
}

string_enum! {
    /// Stored edge classification used by proof-path validation.
    pub enum EdgeClass {
        BaseExact => "base_exact",
        BaseHeuristic => "base_heuristic",
        ReifiedCallsite => "reified_callsite",
        Derived => "derived",
        Test => "test",
        Mock => "mock",
        Mixed => "mixed",
        Unknown => "unknown",
    }
}

string_enum! {
    /// Stored execution context for an edge.
    pub enum EdgeContext {
        Production => "production",
        Test => "test",
        Mock => "mock",
        Mixed => "mixed",
        Unknown => "unknown",
    }
}
