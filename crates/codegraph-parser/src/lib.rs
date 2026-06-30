//! Parser abstraction and initial TypeScript/JavaScript tree-sitter parser.
//!
//! Phase 04 produced parse metadata, source spans, and syntax diagnostics.
//! Phase 05 added basic file/module/declaration/import/export extraction.
//! Phase 06 added conservative syntax-derived call, return, read/write,
//! mutation, and direct flow facts. Phase 07 adds best-effort extended
//! auth/security, event/async, persistence/schema, and test relations as
//! explicitly labeled static heuristics. Vectors, MCP behavior, UI behavior,
//! and benchmark behavior remain out of scope here.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
};

use codegraph_core::{
    decide_local_returns_to_exactness, local_returns_to_identity_input, relation_allows,
    stable_edge_id, stable_entity_id, stable_entity_id_for_kind, stable_fact_identity_key,
    stable_micro_edge_id, stable_micro_node_id, validate_micro_fact_provenance, Edge, EdgeClass,
    EdgeContext, Entity, EntityKind, EvidenceRole, Exactness, FileRecord,
    LocalReturnsToExactnessInput, LocalReturnsToIdentityContractInput, Metadata,
    MicroDerivationKind, MicroEdgeCandidate, MicroEdgeCandidateCapState, MicroEdgeIdentityInput,
    MicroEdgeKind, MicroExactness, MicroFactProvenance, MicroNodeIdentityInput, MicroNodeKind,
    MicroSourceRole, RelationKind, SourceSpan, MVP4_2B_LOCAL_FLOWS_TO_CLAIMABILITY,
    MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION, MVP4_2B_LOCAL_READS_CLAIMABILITY,
    MVP4_2B_LOCAL_READS_EXTRACTION_VERSION, MVP4_2B_LOCAL_WRITES_CLAIMABILITY,
    MVP4_2B_LOCAL_WRITES_EXTRACTION_VERSION, MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION,
    MVP4_2_MICRO_EDGE_PAYLOAD_VERSION, MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tree_sitter::{Node, Parser, Point, Tree};

const MAX_EXTRACTED_LABEL_CHARS: usize = 64;
const MAX_IDENTITY_HASH_CHARS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceLanguage {
    JavaScript,
    Jsx,
    TypeScript,
    Tsx,
    Python,
    Go,
    Rust,
    Java,
    CSharp,
    C,
    Cpp,
    Ruby,
    Php,
}

impl SourceLanguage {
    pub const ALL: &'static [Self] = &[
        Self::JavaScript,
        Self::Jsx,
        Self::TypeScript,
        Self::Tsx,
        Self::Python,
        Self::Go,
        Self::Rust,
        Self::Java,
        Self::CSharp,
        Self::C,
        Self::Cpp,
        Self::Ruby,
        Self::Php,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Python => "python",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Ruby => "ruby",
            Self::Php => "php",
        }
    }

    fn parser_language(self) -> tree_sitter::Language {
        match self {
            Self::JavaScript | Self::Jsx => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        }
    }

    pub const fn is_javascript_family(self) -> bool {
        matches!(
            self,
            Self::JavaScript | Self::Jsx | Self::TypeScript | Self::Tsx
        )
    }
}

impl fmt::Display for SourceLanguage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SourceLanguage {
    type Err = ParseLanguageError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "javascript" | "js" => Ok(Self::JavaScript),
            "jsx" => Ok(Self::Jsx),
            "typescript" | "ts" => Ok(Self::TypeScript),
            "tsx" => Ok(Self::Tsx),
            "python" | "py" => Ok(Self::Python),
            "go" => Ok(Self::Go),
            "rust" | "rs" => Ok(Self::Rust),
            "java" => Ok(Self::Java),
            "csharp" | "c#" | "cs" => Ok(Self::CSharp),
            "c" => Ok(Self::C),
            "cpp" | "c++" | "cc" | "cxx" | "hpp" => Ok(Self::Cpp),
            "ruby" | "rb" => Ok(Self::Ruby),
            "php" => Ok(Self::Php),
            _ => Err(ParseLanguageError(raw.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseLanguageError(String);

impl fmt::Display for ParseLanguageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unsupported parser language: {}", self.0)
    }
}

impl Error for ParseLanguageError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageSupportTier {
    /// Files are discovered and reported, but syntax is not parsed.
    Tier0FileDiscovery,
    /// Tree-sitter syntax parsing and declaration/entity extraction.
    Tier1SyntaxEntities,
    /// Imports, exports, packages, or equivalent namespace facts.
    Tier2ImportsExportsPackages,
    /// Caller/callee extraction is available.
    Tier3CallsCallerCallee,
    /// Compiler or LSP verified resolution can upgrade exactness.
    Tier4CompilerOrLspVerified,
    /// Dataflow, security, or test impact facts are supported.
    Tier5DataflowSecurityTestImpact,
}

impl LanguageSupportTier {
    pub const fn number(self) -> u8 {
        match self {
            Self::Tier0FileDiscovery => 0,
            Self::Tier1SyntaxEntities => 1,
            Self::Tier2ImportsExportsPackages => 2,
            Self::Tier3CallsCallerCallee => 3,
            Self::Tier4CompilerOrLspVerified => 4,
            Self::Tier5DataflowSecurityTestImpact => 5,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Tier0FileDiscovery => "Tier 0: file discovery only",
            Self::Tier1SyntaxEntities => "Tier 1: syntax/entity extraction",
            Self::Tier2ImportsExportsPackages => "Tier 2: imports/exports/packages",
            Self::Tier3CallsCallerCallee => "Tier 3: calls/caller-callee",
            Self::Tier4CompilerOrLspVerified => "Tier 4: compiler/LSP verified resolution",
            Self::Tier5DataflowSecurityTestImpact => "Tier 5: dataflow/security/test impact",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractorCapability {
    pub name: &'static str,
    pub exactness: Exactness,
    pub supported_relations: &'static [RelationKind],
    pub known_limitations: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LanguageFrontendInfo {
    pub language_id: &'static str,
    pub display_name: &'static str,
    pub file_extensions: &'static [&'static str],
    pub support_tier: LanguageSupportTier,
    pub tree_sitter_grammar_available: bool,
    pub compiler_resolver_available: bool,
    pub lsp_resolver_available: bool,
    pub supported_entity_kinds: &'static [EntityKind],
    pub supported_relation_kinds: &'static [RelationKind],
    pub extractors: &'static [ExtractorCapability],
    pub known_limitations: &'static [&'static str],
}

pub trait LanguageFrontend {
    fn info(&self) -> &'static LanguageFrontendInfo;
}

#[derive(Debug, Clone, Copy)]
pub struct StaticLanguageFrontend {
    info: &'static LanguageFrontendInfo,
}

impl StaticLanguageFrontend {
    pub const fn new(info: &'static LanguageFrontendInfo) -> Self {
        Self { info }
    }
}

impl LanguageFrontend for StaticLanguageFrontend {
    fn info(&self) -> &'static LanguageFrontendInfo {
        self.info
    }
}

#[derive(Debug, Clone)]
pub struct FrontendRegistry {
    frontends: Vec<StaticLanguageFrontend>,
}

impl FrontendRegistry {
    pub fn default_registry() -> Self {
        Self {
            frontends: LANGUAGE_FRONTENDS
                .iter()
                .copied()
                .map(StaticLanguageFrontend::new)
                .collect(),
        }
    }

    pub fn frontends(&self) -> &[StaticLanguageFrontend] {
        &self.frontends
    }

    pub fn info_for_language(
        &self,
        language: SourceLanguage,
    ) -> Option<&'static LanguageFrontendInfo> {
        self.frontends
            .iter()
            .map(LanguageFrontend::info)
            .find(|info| info.language_id == language.as_str())
    }

    pub fn info_for_path(&self, path: impl AsRef<Path>) -> Option<&'static LanguageFrontendInfo> {
        let language = detect_language(path)?;
        self.info_for_language(language)
    }
}

pub fn default_frontend_registry() -> FrontendRegistry {
    FrontendRegistry::default_registry()
}

pub fn language_frontends() -> &'static [&'static LanguageFrontendInfo] {
    LANGUAGE_FRONTENDS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceCapabilities {
    pub compiler_resolver_available: bool,
    pub lsp_resolver_available: bool,
    pub supported_languages: Vec<String>,
    pub exactness_when_available: Exactness,
    pub known_limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticResolution {
    pub query: String,
    pub resolved_name: String,
    pub repo_relative_path: String,
    pub source_span: Option<SourceSpan>,
    pub exactness: Exactness,
    pub confidence: f64,
    pub metadata: serde_json::Value,
}

pub type SemanticResult<T> = Result<T, SemanticResolverError>;

#[derive(Debug)]
pub enum SemanticResolverError {
    Unavailable(String),
    Failed(String),
    InvalidResponse(String),
}

impl fmt::Display for SemanticResolverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => {
                write!(formatter, "semantic resolver unavailable: {message}")
            }
            Self::Failed(message) => write!(formatter, "semantic resolver failed: {message}"),
            Self::InvalidResponse(message) => {
                write!(
                    formatter,
                    "semantic resolver returned invalid response: {message}"
                )
            }
        }
    }
}

impl Error for SemanticResolverError {}

pub trait SemanticResolver {
    fn resolve_symbol(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        symbol: &str,
    ) -> SemanticResult<Vec<SemanticResolution>>;

    fn resolve_import(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        import: &str,
    ) -> SemanticResult<Vec<SemanticResolution>>;

    fn resolve_call_target(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        call: &str,
    ) -> SemanticResult<Vec<SemanticResolution>>;

    fn resolve_type(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        symbol: &str,
    ) -> SemanticResult<Vec<SemanticResolution>>;

    fn resolve_references(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        symbol: &str,
    ) -> SemanticResult<Vec<SemanticResolution>>;

    fn workspace_capabilities(&self, repo_root: &Path) -> WorkspaceCapabilities;
}

#[derive(Debug, Clone)]
pub struct TypeScriptSemanticResolver {
    node_executable: PathBuf,
    helper_path: PathBuf,
}

impl TypeScriptSemanticResolver {
    pub fn default_helper() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/typescript-resolver.mjs")
    }

    pub fn new(node_executable: impl Into<PathBuf>, helper_path: impl Into<PathBuf>) -> Self {
        Self {
            node_executable: node_executable.into(),
            helper_path: helper_path.into(),
        }
    }

    pub fn default_node() -> Self {
        Self::new("node", Self::default_helper())
    }

    fn resolve_operation(
        &self,
        operation: &str,
        repo_root: &Path,
        repo_relative_path: &str,
        query: &str,
    ) -> SemanticResult<Vec<SemanticResolution>> {
        if !self.helper_path.exists() {
            return Err(SemanticResolverError::Unavailable(format!(
                "missing TypeScript helper at {}",
                self.helper_path.display()
            )));
        }

        let output = Command::new(&self.node_executable)
            .arg(&self.helper_path)
            .arg(operation)
            .arg(repo_root)
            .arg(repo_relative_path)
            .arg(query)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| SemanticResolverError::Unavailable(error.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            return Err(SemanticResolverError::Failed(format!(
                "{}{}",
                stdout.trim(),
                stderr.trim()
            )));
        }

        #[derive(Deserialize)]
        struct HelperResponse {
            status: String,
            capabilities: Option<WorkspaceCapabilities>,
            resolutions: Option<Vec<SemanticResolution>>,
            message: Option<String>,
        }

        let response: HelperResponse = serde_json::from_str(&stdout)
            .map_err(|error| SemanticResolverError::InvalidResponse(error.to_string()))?;
        if response.status == "unavailable" {
            return Err(SemanticResolverError::Unavailable(
                response
                    .message
                    .unwrap_or_else(|| "TypeScript compiler API unavailable".to_string()),
            ));
        }
        let _capabilities = response.capabilities;
        Ok(response.resolutions.unwrap_or_default())
    }
}

impl Default for TypeScriptSemanticResolver {
    fn default() -> Self {
        Self::default_node()
    }
}

impl SemanticResolver for TypeScriptSemanticResolver {
    fn resolve_symbol(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        symbol: &str,
    ) -> SemanticResult<Vec<SemanticResolution>> {
        self.resolve_operation("resolve_symbol", repo_root, repo_relative_path, symbol)
    }

    fn resolve_import(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        import: &str,
    ) -> SemanticResult<Vec<SemanticResolution>> {
        self.resolve_operation("resolve_import", repo_root, repo_relative_path, import)
    }

    fn resolve_call_target(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        call: &str,
    ) -> SemanticResult<Vec<SemanticResolution>> {
        self.resolve_operation("resolve_call_target", repo_root, repo_relative_path, call)
    }

    fn resolve_type(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        symbol: &str,
    ) -> SemanticResult<Vec<SemanticResolution>> {
        self.resolve_operation("resolve_type", repo_root, repo_relative_path, symbol)
    }

    fn resolve_references(
        &self,
        repo_root: &Path,
        repo_relative_path: &str,
        symbol: &str,
    ) -> SemanticResult<Vec<SemanticResolution>> {
        self.resolve_operation("resolve_references", repo_root, repo_relative_path, symbol)
    }

    fn workspace_capabilities(&self, _repo_root: &Path) -> WorkspaceCapabilities {
        WorkspaceCapabilities {
            compiler_resolver_available: self.helper_path.exists(),
            lsp_resolver_available: false,
            supported_languages: vec!["typescript".to_string(), "tsx".to_string()],
            exactness_when_available: Exactness::CompilerVerified,
            known_limitations: vec![
                "Requires Node and a resolvable typescript package in the target workspace or helper environment"
                    .to_string(),
                "Parser-only indexing does not depend on this resolver".to_string(),
            ],
        }
    }
}

const STRUCTURAL_RELATIONS: &[RelationKind] = &[
    RelationKind::Contains,
    RelationKind::DefinedIn,
    RelationKind::Defines,
    RelationKind::Declares,
    RelationKind::Imports,
    RelationKind::Exports,
];

const JS_TS_RELATIONS: &[RelationKind] = &[
    RelationKind::Contains,
    RelationKind::DefinedIn,
    RelationKind::Defines,
    RelationKind::Declares,
    RelationKind::Imports,
    RelationKind::Exports,
    RelationKind::Calls,
    RelationKind::Callee,
    RelationKind::Argument0,
    RelationKind::Argument1,
    RelationKind::ArgumentN,
    RelationKind::Returns,
    RelationKind::ReturnsTo,
    RelationKind::Reads,
    RelationKind::Writes,
    RelationKind::Mutates,
    RelationKind::AssignedFrom,
    RelationKind::FlowsTo,
    RelationKind::Authorizes,
    RelationKind::ChecksRole,
    RelationKind::ChecksPermission,
    RelationKind::Sanitizes,
    RelationKind::Validates,
    RelationKind::Exposes,
    RelationKind::TrustBoundary,
    RelationKind::SourceOfTaint,
    RelationKind::SinksTo,
    RelationKind::Publishes,
    RelationKind::Emits,
    RelationKind::Consumes,
    RelationKind::ListensTo,
    RelationKind::SubscribesTo,
    RelationKind::Handles,
    RelationKind::Spawns,
    RelationKind::Awaits,
    RelationKind::Migrates,
    RelationKind::ReadsTable,
    RelationKind::WritesTable,
    RelationKind::AltersColumn,
    RelationKind::DependsOnSchema,
    RelationKind::Tests,
    RelationKind::Asserts,
    RelationKind::Mocks,
    RelationKind::Stubs,
    RelationKind::Covers,
    RelationKind::FixturesFor,
];

const JS_TS_ENTITY_KINDS: &[EntityKind] = &[
    EntityKind::File,
    EntityKind::Module,
    EntityKind::Class,
    EntityKind::Interface,
    EntityKind::Function,
    EntityKind::Method,
    EntityKind::Constructor,
    EntityKind::Parameter,
    EntityKind::LocalVariable,
    EntityKind::Import,
    EntityKind::Export,
    EntityKind::CallSite,
    EntityKind::ReturnSite,
    EntityKind::Route,
    EntityKind::Role,
    EntityKind::Sanitizer,
    EntityKind::Validator,
    EntityKind::Event,
    EntityKind::Table,
    EntityKind::Column,
    EntityKind::TestFile,
    EntityKind::TestSuite,
    EntityKind::TestCase,
    EntityKind::Fixture,
    EntityKind::Mock,
    EntityKind::Stub,
    EntityKind::Assertion,
];

const TIER1_ENTITY_KINDS: &[EntityKind] = &[
    EntityKind::File,
    EntityKind::Module,
    EntityKind::Class,
    EntityKind::Interface,
    EntityKind::Trait,
    EntityKind::Enum,
    EntityKind::Function,
    EntityKind::Method,
    EntityKind::Constructor,
    EntityKind::Parameter,
    EntityKind::LocalVariable,
    EntityKind::Import,
    EntityKind::Export,
];

const GENERIC_TIER3_ENTITY_KINDS: &[EntityKind] = &[
    EntityKind::File,
    EntityKind::Module,
    EntityKind::Class,
    EntityKind::Interface,
    EntityKind::Trait,
    EntityKind::Enum,
    EntityKind::Function,
    EntityKind::Method,
    EntityKind::Constructor,
    EntityKind::Parameter,
    EntityKind::LocalVariable,
    EntityKind::Import,
    EntityKind::Export,
    EntityKind::TestFile,
    EntityKind::TestSuite,
    EntityKind::TestCase,
    EntityKind::Fixture,
    EntityKind::CallSite,
    EntityKind::Expression,
];

const GENERIC_TIER3_RELATIONS: &[RelationKind] = &[
    RelationKind::Contains,
    RelationKind::DefinedIn,
    RelationKind::Defines,
    RelationKind::Declares,
    RelationKind::Imports,
    RelationKind::Exports,
    RelationKind::Calls,
    RelationKind::Callee,
    RelationKind::Argument0,
    RelationKind::Argument1,
    RelationKind::ArgumentN,
    RelationKind::FlowsTo,
];

const JS_EXTRACTORS: &[ExtractorCapability] = &[
    ExtractorCapability {
        name: "tree-sitter-basic",
        exactness: Exactness::ParserVerified,
        supported_relations: STRUCTURAL_RELATIONS,
        known_limitations: &["parser-only symbol resolution cannot prove cross-file aliases"],
    },
    ExtractorCapability {
        name: "tree-sitter-core-relations",
        exactness: Exactness::ParserVerified,
        supported_relations: &[
            RelationKind::Calls,
            RelationKind::Callee,
            RelationKind::Argument0,
            RelationKind::Argument1,
            RelationKind::ArgumentN,
            RelationKind::Returns,
            RelationKind::ReturnsTo,
            RelationKind::Reads,
            RelationKind::Writes,
            RelationKind::Mutates,
            RelationKind::AssignedFrom,
            RelationKind::FlowsTo,
        ],
        known_limitations: &["unresolved call targets are downgraded to static_heuristic"],
    },
    ExtractorCapability {
        name: "tree-sitter-extended-heuristic",
        exactness: Exactness::StaticHeuristic,
        supported_relations: &[
            RelationKind::Authorizes,
            RelationKind::ChecksRole,
            RelationKind::ChecksPermission,
            RelationKind::Sanitizes,
            RelationKind::Validates,
            RelationKind::Exposes,
            RelationKind::TrustBoundary,
            RelationKind::SourceOfTaint,
            RelationKind::SinksTo,
            RelationKind::Publishes,
            RelationKind::Emits,
            RelationKind::Consumes,
            RelationKind::ListensTo,
            RelationKind::SubscribesTo,
            RelationKind::Handles,
            RelationKind::Spawns,
            RelationKind::Awaits,
            RelationKind::Migrates,
            RelationKind::ReadsTable,
            RelationKind::WritesTable,
            RelationKind::AltersColumn,
            RelationKind::DependsOnSchema,
            RelationKind::Tests,
            RelationKind::Asserts,
            RelationKind::Mocks,
            RelationKind::Stubs,
            RelationKind::Covers,
            RelationKind::FixturesFor,
        ],
        known_limitations: &["framework pattern edges are heuristic and never compiler verified"],
    },
];

const TS_EXTRACTORS: &[ExtractorCapability] = &[
    ExtractorCapability {
        name: "tree-sitter-basic",
        exactness: Exactness::ParserVerified,
        supported_relations: STRUCTURAL_RELATIONS,
        known_limitations: &["parser-only symbol resolution cannot prove cross-file aliases"],
    },
    ExtractorCapability {
        name: "tree-sitter-core-relations",
        exactness: Exactness::ParserVerified,
        supported_relations: &[
            RelationKind::Calls,
            RelationKind::Callee,
            RelationKind::Argument0,
            RelationKind::Argument1,
            RelationKind::ArgumentN,
            RelationKind::Returns,
            RelationKind::ReturnsTo,
            RelationKind::Reads,
            RelationKind::Writes,
            RelationKind::Mutates,
            RelationKind::AssignedFrom,
            RelationKind::FlowsTo,
        ],
        known_limitations: &["unresolved call targets are downgraded to static_heuristic"],
    },
    ExtractorCapability {
        name: "tree-sitter-extended-heuristic",
        exactness: Exactness::StaticHeuristic,
        supported_relations: &[
            RelationKind::Authorizes,
            RelationKind::ChecksRole,
            RelationKind::ChecksPermission,
            RelationKind::Sanitizes,
            RelationKind::Validates,
            RelationKind::Exposes,
            RelationKind::TrustBoundary,
            RelationKind::SourceOfTaint,
            RelationKind::SinksTo,
            RelationKind::Publishes,
            RelationKind::Emits,
            RelationKind::Consumes,
            RelationKind::ListensTo,
            RelationKind::SubscribesTo,
            RelationKind::Handles,
            RelationKind::Spawns,
            RelationKind::Awaits,
            RelationKind::Migrates,
            RelationKind::ReadsTable,
            RelationKind::WritesTable,
            RelationKind::AltersColumn,
            RelationKind::DependsOnSchema,
            RelationKind::Tests,
            RelationKind::Asserts,
            RelationKind::Mocks,
            RelationKind::Stubs,
            RelationKind::Covers,
            RelationKind::FixturesFor,
        ],
        known_limitations: &["framework pattern edges are heuristic and never compiler verified"],
    },
    ExtractorCapability {
        name: "typescript-compiler-resolver",
        exactness: Exactness::CompilerVerified,
        supported_relations: &[
            RelationKind::Imports,
            RelationKind::Exports,
            RelationKind::AliasOf,
            RelationKind::AliasedBy,
            RelationKind::Calls,
        ],
        known_limitations: &["optional Node/TypeScript helper; parser-only indexing still works"],
    },
];

const GENERIC_TIER3_EXTRACTORS: &[ExtractorCapability] = &[
    ExtractorCapability {
        name: "tree-sitter-language-frontend",
        exactness: Exactness::ParserVerified,
        supported_relations: STRUCTURAL_RELATIONS,
        known_limitations: &["imports/exports are syntax facts, not resolved package graph facts"],
    },
    ExtractorCapability {
        name: "tree-sitter-conservative-call-extractor",
        exactness: Exactness::ParserVerified,
        supported_relations: &[
            RelationKind::Calls,
            RelationKind::Callee,
            RelationKind::Argument0,
            RelationKind::Argument1,
            RelationKind::ArgumentN,
            RelationKind::FlowsTo,
        ],
        known_limitations: &[
            "same-scope calls are parser verified",
            "unresolved or cross-file calls are retained as static_heuristic placeholders",
        ],
    },
];

const GENERIC_TIER1_EXTRACTORS: &[ExtractorCapability] = &[ExtractorCapability {
    name: "tree-sitter-language-frontend",
    exactness: Exactness::ParserVerified,
    supported_relations: &[
        RelationKind::Contains,
        RelationKind::DefinedIn,
        RelationKind::Defines,
        RelationKind::Declares,
        RelationKind::Imports,
        RelationKind::Exports,
    ],
    known_limitations: &["syntax/entities only; package/import resolution remains unsupported"],
}];

const JS_LIMITATIONS: &[&str] = &[
    "parser-verified local calls can be exact only when target declarations are in scope",
    "cross-file import alias verification is not enabled for JavaScript in this tier",
];
const TS_LIMITATIONS: &[&str] = &[
    "compiler resolver is optional and unavailable when Node or TypeScript is absent",
    "parser-only fallback preserves exactness labels and does not fake compiler proof",
];
const TIER3_LIMITATIONS: &[&str] = &[
    "caller/callee extraction is conservative and parser-level only",
    "cross-file calls are not compiler verified",
    "dataflow, security, and test impact remain explicitly unsupported",
];
const TIER1_LIMITATIONS: &[&str] = &[
    "syntax/entity extraction only",
    "imports are recorded only when the grammar exposes explicit import/use/include nodes",
    "calls, dataflow, security, and test impact are explicitly unsupported in this tier",
];

static LANGUAGE_FRONTENDS: &[&LanguageFrontendInfo] = &[
    &LanguageFrontendInfo {
        language_id: "javascript",
        display_name: "JavaScript",
        file_extensions: &["js", "mjs", "cjs"],
        support_tier: LanguageSupportTier::Tier5DataflowSecurityTestImpact,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: JS_TS_ENTITY_KINDS,
        supported_relation_kinds: JS_TS_RELATIONS,
        extractors: JS_EXTRACTORS,
        known_limitations: JS_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "jsx",
        display_name: "JSX",
        file_extensions: &["jsx"],
        support_tier: LanguageSupportTier::Tier5DataflowSecurityTestImpact,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: JS_TS_ENTITY_KINDS,
        supported_relation_kinds: JS_TS_RELATIONS,
        extractors: JS_EXTRACTORS,
        known_limitations: JS_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "typescript",
        display_name: "TypeScript",
        file_extensions: &["ts", "mts", "cts"],
        support_tier: LanguageSupportTier::Tier5DataflowSecurityTestImpact,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: true,
        lsp_resolver_available: false,
        supported_entity_kinds: JS_TS_ENTITY_KINDS,
        supported_relation_kinds: JS_TS_RELATIONS,
        extractors: TS_EXTRACTORS,
        known_limitations: TS_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "tsx",
        display_name: "TSX",
        file_extensions: &["tsx"],
        support_tier: LanguageSupportTier::Tier5DataflowSecurityTestImpact,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: true,
        lsp_resolver_available: false,
        supported_entity_kinds: JS_TS_ENTITY_KINDS,
        supported_relation_kinds: JS_TS_RELATIONS,
        extractors: TS_EXTRACTORS,
        known_limitations: TS_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "python",
        display_name: "Python",
        file_extensions: &["py"],
        support_tier: LanguageSupportTier::Tier3CallsCallerCallee,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: GENERIC_TIER3_ENTITY_KINDS,
        supported_relation_kinds: GENERIC_TIER3_RELATIONS,
        extractors: GENERIC_TIER3_EXTRACTORS,
        known_limitations: TIER3_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "go",
        display_name: "Go",
        file_extensions: &["go"],
        support_tier: LanguageSupportTier::Tier3CallsCallerCallee,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: GENERIC_TIER3_ENTITY_KINDS,
        supported_relation_kinds: GENERIC_TIER3_RELATIONS,
        extractors: GENERIC_TIER3_EXTRACTORS,
        known_limitations: TIER3_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "rust",
        display_name: "Rust",
        file_extensions: &["rs"],
        support_tier: LanguageSupportTier::Tier3CallsCallerCallee,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: GENERIC_TIER3_ENTITY_KINDS,
        supported_relation_kinds: GENERIC_TIER3_RELATIONS,
        extractors: GENERIC_TIER3_EXTRACTORS,
        known_limitations: TIER3_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "java",
        display_name: "Java",
        file_extensions: &["java"],
        support_tier: LanguageSupportTier::Tier1SyntaxEntities,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: TIER1_ENTITY_KINDS,
        supported_relation_kinds: STRUCTURAL_RELATIONS,
        extractors: GENERIC_TIER1_EXTRACTORS,
        known_limitations: TIER1_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "csharp",
        display_name: "C#",
        file_extensions: &["cs"],
        support_tier: LanguageSupportTier::Tier1SyntaxEntities,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: TIER1_ENTITY_KINDS,
        supported_relation_kinds: STRUCTURAL_RELATIONS,
        extractors: GENERIC_TIER1_EXTRACTORS,
        known_limitations: TIER1_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "c",
        display_name: "C",
        file_extensions: &["c", "h"],
        support_tier: LanguageSupportTier::Tier1SyntaxEntities,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: TIER1_ENTITY_KINDS,
        supported_relation_kinds: STRUCTURAL_RELATIONS,
        extractors: GENERIC_TIER1_EXTRACTORS,
        known_limitations: TIER1_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "cpp",
        display_name: "C++",
        file_extensions: &["cc", "cpp", "cxx", "hpp", "hh", "hxx"],
        support_tier: LanguageSupportTier::Tier1SyntaxEntities,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: TIER1_ENTITY_KINDS,
        supported_relation_kinds: STRUCTURAL_RELATIONS,
        extractors: GENERIC_TIER1_EXTRACTORS,
        known_limitations: TIER1_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "ruby",
        display_name: "Ruby",
        file_extensions: &["rb"],
        support_tier: LanguageSupportTier::Tier1SyntaxEntities,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: TIER1_ENTITY_KINDS,
        supported_relation_kinds: STRUCTURAL_RELATIONS,
        extractors: GENERIC_TIER1_EXTRACTORS,
        known_limitations: TIER1_LIMITATIONS,
    },
    &LanguageFrontendInfo {
        language_id: "php",
        display_name: "PHP",
        file_extensions: &["php"],
        support_tier: LanguageSupportTier::Tier1SyntaxEntities,
        tree_sitter_grammar_available: true,
        compiler_resolver_available: false,
        lsp_resolver_available: false,
        supported_entity_kinds: TIER1_ENTITY_KINDS,
        supported_relation_kinds: STRUCTURAL_RELATIONS,
        extractors: GENERIC_TIER1_EXTRACTORS,
        known_limitations: TIER1_LIMITATIONS,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxNodeRef {
    pub kind: String,
    pub is_named: bool,
    pub is_error: bool,
    pub is_missing: bool,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_position: SourcePoint,
    pub end_position: SourcePoint,
    pub source_span: SourceSpan,
    pub child_count: usize,
    pub named_child_count: usize,
}

impl SyntaxNodeRef {
    fn from_node(repo_relative_path: &str, node: Node<'_>) -> Self {
        let start_position = SourcePoint::from_tree_sitter(node.start_position());
        let end_position = SourcePoint::from_tree_sitter(node.end_position());
        Self {
            kind: node.kind().to_string(),
            is_named: node.is_named(),
            is_error: node.is_error(),
            is_missing: node.is_missing(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            source_span: SourceSpan::with_columns(
                repo_relative_path,
                start_position.line,
                start_position.column,
                end_position.line,
                end_position.column,
            ),
            start_position,
            end_position,
            child_count: node.child_count(),
            named_child_count: node.named_child_count(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePoint {
    pub line: u32,
    pub column: u32,
}

impl SourcePoint {
    fn from_tree_sitter(point: Point) -> Self {
        Self {
            line: point.row.saturating_add(1) as u32,
            column: point.column.saturating_add(1) as u32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub message: String,
    pub node: SyntaxNodeRef,
}

#[derive(Debug)]
pub struct ParsedFile {
    pub repo_relative_path: String,
    pub language: SourceLanguage,
    pub byte_len: usize,
    pub line_count: usize,
    pub root_node: SyntaxNodeRef,
    pub diagnostics: Vec<ParseDiagnostic>,
    tree: Tree,
}

impl ParsedFile {
    pub fn has_syntax_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BasicExtraction {
    pub file: FileRecord,
    pub entities: Vec<Entity>,
    pub edges: Vec<Edge>,
}

impl BasicExtraction {
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

pub trait LanguageParser {
    fn parse(&self, repo_relative_path: &str, source: &str) -> ParseResult<Option<ParsedFile>>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TreeSitterParser;

impl TreeSitterParser {
    pub fn parse_source(
        &self,
        repo_relative_path: &str,
        source: &str,
        language: SourceLanguage,
    ) -> ParseResult<ParsedFile> {
        let mut parser = Parser::new();
        parser
            .set_language(&language.parser_language())
            .map_err(|error| ParseError::LanguageLoad {
                language,
                message: error.to_string(),
            })?;

        let tree = parser.parse(source, None).ok_or(ParseError::ParseFailed {
            path: repo_relative_path.to_string(),
            language,
        })?;
        let root_node = SyntaxNodeRef::from_node(repo_relative_path, tree.root_node());
        let diagnostics = collect_diagnostics(repo_relative_path, tree.root_node());

        Ok(ParsedFile {
            repo_relative_path: normalize_repo_relative_path(repo_relative_path),
            language,
            byte_len: source.len(),
            line_count: source.lines().count(),
            root_node,
            diagnostics,
            tree,
        })
    }
}

impl LanguageParser for TreeSitterParser {
    fn parse(&self, repo_relative_path: &str, source: &str) -> ParseResult<Option<ParsedFile>> {
        let Some(language) = detect_language_with_source(repo_relative_path, source) else {
            return Ok(None);
        };
        self.parse_source(repo_relative_path, source, language)
            .map(Some)
    }
}

pub fn detect_language(path: impl AsRef<Path>) -> Option<SourceLanguage> {
    let extension = path.as_ref().extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "js" | "mjs" | "cjs" => Some(SourceLanguage::JavaScript),
        "jsx" => Some(SourceLanguage::Jsx),
        "ts" | "mts" | "cts" => Some(SourceLanguage::TypeScript),
        "tsx" => Some(SourceLanguage::Tsx),
        "py" => Some(SourceLanguage::Python),
        "go" => Some(SourceLanguage::Go),
        "rs" => Some(SourceLanguage::Rust),
        "java" => Some(SourceLanguage::Java),
        "cs" => Some(SourceLanguage::CSharp),
        "c" | "h" => Some(SourceLanguage::C),
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Some(SourceLanguage::Cpp),
        "rb" => Some(SourceLanguage::Ruby),
        "php" => Some(SourceLanguage::Php),
        _ => None,
    }
}

pub fn detect_language_with_source(path: impl AsRef<Path>, source: &str) -> Option<SourceLanguage> {
    let path = path.as_ref();
    detect_language(path).or_else(|| {
        path.extension()
            .is_none()
            .then(|| detect_language_from_shebang(source))
            .flatten()
    })
}

fn detect_language_from_shebang(source: &str) -> Option<SourceLanguage> {
    let first_line = source.lines().next()?.trim_start_matches('\u{feff}');
    let shebang = first_line.strip_prefix("#!")?.to_ascii_lowercase();
    let mut tokens = shebang
        .split_whitespace()
        .filter_map(|token| token.rsplit(['/', '\\']).next())
        .map(|token| token.trim_end_matches(".exe"))
        .filter(|token| {
            !token.is_empty()
                && *token != "env"
                && *token != "-s"
                && *token != "--"
                && !token.starts_with('-')
        });

    tokens.find_map(|token| {
        if token.starts_with("python") || token.starts_with("pypy") {
            Some(SourceLanguage::Python)
        } else if token == "ts-node" || token.starts_with("ts-node-") || token == "tsx" {
            Some(SourceLanguage::TypeScript)
        } else if token == "node"
            || token.starts_with("nodejs")
            || token == "deno"
            || token == "bun"
        {
            Some(SourceLanguage::JavaScript)
        } else if token.starts_with("ruby") {
            Some(SourceLanguage::Ruby)
        } else if token.starts_with("php") {
            Some(SourceLanguage::Php)
        } else {
            None
        }
    })
}

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug)]
pub enum ParseError {
    LanguageLoad {
        language: SourceLanguage,
        message: String,
    },
    ParseFailed {
        path: String,
        language: SourceLanguage,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LanguageLoad { language, message } => {
                write!(formatter, "failed to load {language} parser: {message}")
            }
            Self::ParseFailed { path, language } => {
                write!(formatter, "failed to parse {path} as {language}")
            }
        }
    }
}

impl Error for ParseError {}

fn collect_diagnostics(repo_relative_path: &str, root: Node<'_>) -> Vec<ParseDiagnostic> {
    let mut diagnostics = Vec::new();
    collect_diagnostics_inner(repo_relative_path, root, &mut diagnostics);
    diagnostics
}

fn collect_diagnostics_inner(
    repo_relative_path: &str,
    node: Node<'_>,
    diagnostics: &mut Vec<ParseDiagnostic>,
) {
    if node.is_error() || node.is_missing() {
        let label = if node.is_missing() {
            format!("missing {}", node.kind())
        } else {
            format!("syntax error at {}", node.kind())
        };
        diagnostics.push(ParseDiagnostic {
            message: label,
            node: SyntaxNodeRef::from_node(repo_relative_path, node),
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_diagnostics_inner(repo_relative_path, child, diagnostics);
    }
}

fn normalize_repo_relative_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

pub fn extract_entities_and_relations(parsed: &ParsedFile, source: &str) -> BasicExtraction {
    if parsed.language.is_javascript_family() {
        BasicEntityExtractor::new(parsed, source).extract()
    } else {
        GenericLanguageExtractor::new(parsed, source).extract()
    }
}

pub fn extract_entities_and_core_relations(parsed: &ParsedFile, source: &str) -> BasicExtraction {
    extract_entities_and_relations(parsed, source)
}

pub fn extract_basic_entities(parsed: &ParsedFile, source: &str) -> BasicExtraction {
    extract_entities_and_relations(parsed, source)
}

const MVP4_TS_MICRO_NODE_ROW_SCHEMA_VERSION: u32 = 1;
const MVP4_TS_MICRO_NODE_PAYLOAD_VERSION: u32 = 1;
const MVP4_TS_MICRO_NODE_EXTRACTION_VERSION: &str = "mvp4.1-typescript-micro-nodes-v1";
const MVP4_TS_CORE_DECLARATION_EXTRACTOR: &str = "mvp4.1-typescript-core-declarations-v1";
const MVP4_TS_OPERATION_SITE_EXTRACTOR: &str = "mvp4.1-typescript-operation-sites-v1";
const MVP4_TS_PROPERTY_ACCESS_EXTRACTOR: &str = "mvp4.1-typescript-property-access-v1";
const MVP4_TS_VALUE_USE_EXTRACTOR: &str = "mvp4.2b-typescript-value-use-v1";
const MVP4_TS_MICRO_NODE_FRONTEND: &str = "tree-sitter-typescript";
const MVP4_TS_MICRO_NODE_LANGUAGE: &str = "typescript";
const MVP4_TS_MAX_MICRO_NODES_PER_FUNCTION: usize = 2532;
const MVP4_TS_MAX_MICRO_NODES_PER_FILE: usize = 2532;
const MVP4_TS_MAX_FUNCTION_FRAME_PER_FUNCTION: usize = 4;
const MVP4_TS_MAX_PARAMETER_PER_FUNCTION: usize = 81;
const MVP4_TS_MAX_LOCAL_BINDING_PER_FUNCTION: usize = 362;
const MVP4_TS_MAX_ASSIGNMENT_SITE_PER_FUNCTION: usize = 541;
const MVP4_TS_MAX_RETURN_SITE_PER_FUNCTION: usize = 3;
const MVP4_TS_MAX_CALL_SITE_PER_FUNCTION: usize = 300;
const MVP4_TS_MAX_PROPERTY_ACCESS_PER_FUNCTION: usize = 720;
const MVP4_TS_MAX_VALUE_USE_PER_FUNCTION: usize = 720;
const MVP4_TS_MAX_NAME_OR_LITERAL_BYTES: usize = 311;
const MVP4_TS_MAX_PARSER_RECOVERY_DERIVED_NODES: usize = 8;
const MVP4_TS_MAX_OMISSION_DETAILS_IN_COMPACT_STATUS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptScopeDiscoveryReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub source_role_reason: String,
    pub source_role_source: String,
    pub parser_status: String,
    pub functions: Vec<Mvp4TypeScriptFunctionScope>,
    pub production_persistence_enabled: bool,
    pub production_micro_nodes_emitted: u64,
    pub micro_edges_emitted: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptFunctionScope {
    pub function_identity: String,
    pub function_name: String,
    pub function_kind: String,
    pub qualifier_path: Vec<String>,
    pub source_span: SourceSpan,
    pub body_span: SourceSpan,
    pub source_role: MicroSourceRole,
    pub parse_recovery: bool,
    pub claimability: String,
    pub scope_path: Vec<String>,
    pub structural_ast_path: Vec<String>,
    pub parameter_scope: Mvp4TypeScriptLexicalScope,
    pub lexical_scopes: Vec<Mvp4TypeScriptLexicalScope>,
    pub existing_entity_link: Option<Mvp4TypeScriptEntityLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptLexicalScope {
    pub scope_kind: String,
    pub scope_path: Vec<String>,
    pub source_span: SourceSpan,
    pub structural_ast_path: Vec<String>,
    pub parse_recovery: bool,
    pub bindings: Vec<Mvp4TypeScriptScopeBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptScopeBinding {
    pub name: String,
    pub binding_kind: String,
    pub declaration_kind: Option<String>,
    pub source_span: SourceSpan,
    pub scope_path: Vec<String>,
    pub structural_ast_path: Vec<String>,
    pub parse_recovery: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptEntityLink {
    pub entity_id: String,
    pub entity_kind: EntityKind,
    pub exactness: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptCoreDeclarationCandidateReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<Mvp4TypeScriptMicroNodeCandidate>,
    pub unsupported_boundaries: Vec<Mvp4TypeScriptUnsupportedDeclarationBoundary>,
    pub duplicate_candidate_count: u64,
    pub source_span_failures: u64,
    pub binding_overclaim_count: u64,
    pub production_persistence_enabled: bool,
    pub production_micro_node_rows: u64,
    pub micro_edge_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptOperationSiteCandidateReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<Mvp4TypeScriptMicroNodeCandidate>,
    pub duplicate_candidate_count: u64,
    pub source_span_failures: u64,
    pub callee_target_overclaim_count: u64,
    pub write_flow_overclaim_count: u64,
    pub production_persistence_enabled: bool,
    pub production_micro_node_rows: u64,
    pub micro_edge_rows: u64,
    pub flow_proof_activated: bool,
}

/// MVP4.2b read/use endpoint candidates (`ValueUse`). Existence-only,
/// source-spanned identifier read occurrences inside a function body; the
/// missing endpoint for `LOCAL_READS` and ordered local flow. Binding identity
/// stays unresolved until the local binding resolver gate (B2), so
/// `symbol_binding_id` is always `None` and `binding_overclaim_count` must be 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptValueUseCandidateReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<Mvp4TypeScriptMicroNodeCandidate>,
    pub duplicate_candidate_count: u64,
    pub source_span_failures: u64,
    pub binding_overclaim_count: u64,
    pub production_persistence_enabled: bool,
    pub production_micro_node_rows: u64,
    pub micro_edge_rows: u64,
    pub flow_proof_activated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptPropertyAccessCandidateReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<Mvp4TypeScriptMicroNodeCandidate>,
    pub duplicate_candidate_count: u64,
    pub source_span_failures: u64,
    pub binding_overclaim_count: u64,
    pub semantic_overclaim_count: u64,
    pub production_persistence_enabled: bool,
    pub production_micro_node_rows: u64,
    pub micro_edge_rows: u64,
    pub flow_proof_activated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptInMemoryInventoryReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<Mvp4TypeScriptMicroNodeCandidate>,
    pub omitted_count: u64,
    pub cap_hits: Vec<Mvp4TypeScriptMicroNodeCapHit>,
    pub completeness_label: String,
    pub duplicate_candidate_count: u64,
    pub stable_id_collision_count: u64,
    pub claimable_node_missing_span_count: u64,
    pub source_span_failures: u64,
    pub binding_overclaim_count: u64,
    pub semantic_overclaim_count: u64,
    pub production_persistence_enabled: bool,
    pub production_micro_node_rows: u64,
    pub micro_edge_rows: u64,
    pub flow_proof_activated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptLocalReturnsToInMemoryReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<MicroEdgeCandidate>,
    pub diagnostics: Vec<Mvp4TypeScriptMicroEdgeDiagnostic>,
    pub omitted_edge_count: u64,
    pub completeness_label: String,
    pub duplicate_candidate_count: u64,
    pub cross_function_edge_count: u64,
    pub cross_file_edge_count: u64,
    pub claimable_edge_missing_span_count: u64,
    pub claimable_edge_missing_provenance_count: u64,
    pub flow_semantic_overclaim_count: u64,
    pub production_micro_edge_rows: u64,
    pub local_flow_packet_rows: u64,
    pub mutation_proof_activated: bool,
    pub flow_proof_activated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptMicroFlowExtractionContext {
    pub inventory: Mvp4TypeScriptInMemoryInventoryReport,
    pub persistable_value_uses: Vec<Mvp4TypeScriptMicroNodeCandidate>,
    pub micro_edges: Mvp4TypeScriptLocalReturnsToInMemoryReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptMicroEdgeDiagnostic {
    pub diagnostic_kind: String,
    pub classification: String,
    pub reason: String,
    pub head_micro_node_id: Option<String>,
    pub tail_micro_node_id: Option<String>,
    pub function_identity: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub missing_requirements: Vec<String>,
    pub omitted_count: u64,
    pub recommended_action_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptMicroNodeCapPolicy {
    pub max_micro_nodes_per_function: usize,
    pub max_micro_nodes_per_file: usize,
    pub per_kind_nodes_per_function: BTreeMap<MicroNodeKind, usize>,
    pub max_name_or_literal_bytes: usize,
    pub max_parser_recovery_derived_nodes: usize,
    pub max_omission_details_in_compact_status: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptMicroNodeCapHit {
    pub cap_kind: String,
    pub function_identity: Option<String>,
    pub node_kind: Option<MicroNodeKind>,
    pub retained_count: usize,
    pub omitted_count: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptMicroNodeCandidate {
    pub micro_node_id: String,
    pub node_kind: MicroNodeKind,
    pub repo_relative_path: String,
    pub language: String,
    pub frontend: String,
    pub enclosing_function_identity: String,
    pub function_entity_id: Option<String>,
    pub function_kind: Option<String>,
    pub scope_path: Vec<String>,
    pub structural_ast_path: Vec<String>,
    pub occurrence_index: u32,
    pub source_span: SourceSpan,
    pub name_or_literal: String,
    pub symbol_binding_id: Option<String>,
    pub source_role: MicroSourceRole,
    pub exactness: MicroExactness,
    pub claimability: String,
    pub parse_recovery: bool,
    pub declaration_kind: Option<String>,
    pub row_schema_version: u32,
    pub payload_version: u32,
    pub extraction_version: String,
    pub provenance: MicroFactProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptUnsupportedDeclarationBoundary {
    pub boundary_kind: String,
    pub reason: String,
    pub source_span: SourceSpan,
    pub scope_path: Vec<String>,
    pub structural_ast_path: Vec<String>,
    pub exactness: MicroExactness,
    pub claimability: String,
}

pub fn discover_mvp4_typescript_scopes(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptScopeDiscoveryReport {
    let source_role = mvp4_typescript_source_role(&parsed.repo_relative_path);
    let parser_status = if parsed.has_syntax_errors() {
        "parsed_with_syntax_diagnostics"
    } else {
        "parsed"
    }
    .to_string();

    let mut report = Mvp4TypeScriptScopeDiscoveryReport {
        status: "complete".to_string(),
        eligible: true,
        exclusion_reason: None,
        source_role: source_role.role,
        source_role_reason: source_role.reason.to_string(),
        source_role_source: source_role.source.to_string(),
        parser_status,
        functions: Vec::new(),
        production_persistence_enabled: false,
        production_micro_nodes_emitted: 0,
        micro_edges_emitted: 0,
    };

    if parsed.language != SourceLanguage::TypeScript {
        report.status = "excluded".to_string();
        report.eligible = false;
        report.exclusion_reason = Some("not_typescript_language".to_string());
        return report;
    }
    if !is_mvp4_exact_typescript_ts_path(&parsed.repo_relative_path) {
        report.status = "excluded".to_string();
        report.eligible = false;
        report.exclusion_reason = Some("not_exact_dot_ts_source_file".to_string());
        return report;
    }
    if source_role.role != MicroSourceRole::Production {
        report.status = "excluded".to_string();
        report.eligible = false;
        report.exclusion_reason = Some(format!("source_role_{}", source_role.role.as_str()));
        return report;
    }

    let extraction = extract_entities_and_relations(parsed, source);
    let mut class_stack = Vec::new();
    let mut function_stack = Vec::new();
    collect_mvp4_ts_function_scopes(
        parsed.tree().root_node(),
        parsed,
        source,
        &extraction.entities,
        &mut class_stack,
        &mut function_stack,
        &mut report.functions,
    );
    report.functions.sort_by(|left, right| {
        left.scope_path
            .cmp(&right.scope_path)
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
    });
    report
}

pub fn emit_mvp4_typescript_core_declaration_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptCoreDeclarationCandidateReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    emit_mvp4_typescript_core_declaration_candidates_with_scopes(parsed, source, &scope_report)
}

fn emit_mvp4_typescript_core_declaration_candidates_with_scopes(
    parsed: &ParsedFile,
    source: &str,
    scope_report: &Mvp4TypeScriptScopeDiscoveryReport,
) -> Mvp4TypeScriptCoreDeclarationCandidateReport {
    let mut report = Mvp4TypeScriptCoreDeclarationCandidateReport {
        status: scope_report.status.clone(),
        eligible: scope_report.eligible,
        exclusion_reason: scope_report.exclusion_reason.clone(),
        source_role: scope_report.source_role,
        parser_status: scope_report.parser_status.clone(),
        candidates: Vec::new(),
        unsupported_boundaries: Vec::new(),
        duplicate_candidate_count: 0,
        source_span_failures: 0,
        binding_overclaim_count: 0,
        production_persistence_enabled: false,
        production_micro_node_rows: 0,
        micro_edge_rows: 0,
    };

    if !scope_report.eligible {
        return report;
    }

    let mut occurrence_counts = BTreeMap::<(MicroNodeKind, Vec<String>, String), u32>::new();
    for function in &scope_report.functions {
        push_mvp4_ts_candidate(
            parsed,
            function,
            MicroNodeKind::FunctionFrame,
            MVP4_TS_CORE_DECLARATION_EXTRACTOR,
            &function.scope_path,
            &function.structural_ast_path,
            &function.source_span,
            &function.function_name,
            function
                .existing_entity_link
                .as_ref()
                .map(|link| link.entity_id.clone()),
            Some(function.function_kind.clone()),
            None,
            function.parse_recovery,
            &mut occurrence_counts,
            &mut report.candidates,
        );

        for binding in &function.parameter_scope.bindings {
            push_mvp4_ts_candidate(
                parsed,
                function,
                MicroNodeKind::Parameter,
                MVP4_TS_CORE_DECLARATION_EXTRACTOR,
                &binding.scope_path,
                &binding.structural_ast_path,
                &binding.source_span,
                &binding.name,
                None,
                Some(function.function_kind.clone()),
                binding.declaration_kind.clone(),
                binding.parse_recovery,
                &mut occurrence_counts,
                &mut report.candidates,
            );
        }

        for scope in &function.lexical_scopes {
            for binding in &scope.bindings {
                push_mvp4_ts_candidate(
                    parsed,
                    function,
                    MicroNodeKind::LocalBinding,
                    MVP4_TS_CORE_DECLARATION_EXTRACTOR,
                    &binding.scope_path,
                    &binding.structural_ast_path,
                    &binding.source_span,
                    &binding.name,
                    None,
                    Some(function.function_kind.clone()),
                    binding.declaration_kind.clone(),
                    binding.parse_recovery,
                    &mut occurrence_counts,
                    &mut report.candidates,
                );
            }
        }
    }

    report.unsupported_boundaries =
        collect_mvp4_ts_unsupported_core_declaration_boundaries(parsed, source);
    report.candidates.sort_by(|left, right| {
        left.source_span
            .start_line
            .cmp(&right.source_span.start_line)
            .then_with(|| {
                left.source_span
                    .start_column
                    .cmp(&right.source_span.start_column)
            })
            .then_with(|| left.node_kind.cmp(&right.node_kind))
            .then_with(|| left.name_or_literal.cmp(&right.name_or_literal))
            .then_with(|| left.micro_node_id.cmp(&right.micro_node_id))
    });

    let mut ids = BTreeSet::new();
    for candidate in &report.candidates {
        if !ids.insert(candidate.micro_node_id.clone()) {
            report.duplicate_candidate_count += 1;
        }
        if !mvp4_source_span_is_bounded(&candidate.source_span) {
            report.source_span_failures += 1;
        }
        if matches!(
            candidate.node_kind,
            MicroNodeKind::Parameter | MicroNodeKind::LocalBinding
        ) && candidate.symbol_binding_id.is_some()
        {
            report.binding_overclaim_count += 1;
        }
    }
    report
}

pub fn emit_mvp4_typescript_operation_site_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptOperationSiteCandidateReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    emit_mvp4_typescript_operation_site_candidates_with_scopes(parsed, source, &scope_report)
}

fn emit_mvp4_typescript_operation_site_candidates_with_scopes(
    parsed: &ParsedFile,
    source: &str,
    scope_report: &Mvp4TypeScriptScopeDiscoveryReport,
) -> Mvp4TypeScriptOperationSiteCandidateReport {
    let mut report = Mvp4TypeScriptOperationSiteCandidateReport {
        status: scope_report.status.clone(),
        eligible: scope_report.eligible,
        exclusion_reason: scope_report.exclusion_reason.clone(),
        source_role: scope_report.source_role,
        parser_status: scope_report.parser_status.clone(),
        candidates: Vec::new(),
        duplicate_candidate_count: 0,
        source_span_failures: 0,
        callee_target_overclaim_count: 0,
        write_flow_overclaim_count: 0,
        production_persistence_enabled: false,
        production_micro_node_rows: 0,
        micro_edge_rows: 0,
        flow_proof_activated: false,
    };

    if !scope_report.eligible {
        return report;
    }

    let root = parsed.tree().root_node();
    let function_nodes_by_span =
        index_mvp4_ts_function_nodes_by_span(root, &parsed.repo_relative_path);
    let mut occurrence_counts = BTreeMap::<(MicroNodeKind, Vec<String>, String), u32>::new();
    for function in &scope_report.functions {
        let Some(function_node) = function_nodes_by_span
            .get(&mvp4_source_span_key(&function.source_span))
            .copied()
        else {
            continue;
        };
        let Some(body_node) = function_node.child_by_field_name("body") else {
            continue;
        };
        collect_mvp4_ts_operation_site_candidates(
            body_node,
            parsed,
            source,
            function,
            &mut occurrence_counts,
            &mut report.candidates,
        );
    }

    report.candidates.sort_by(|left, right| {
        left.source_span
            .start_line
            .cmp(&right.source_span.start_line)
            .then_with(|| {
                left.source_span
                    .start_column
                    .cmp(&right.source_span.start_column)
            })
            .then_with(|| left.node_kind.cmp(&right.node_kind))
            .then_with(|| left.name_or_literal.cmp(&right.name_or_literal))
            .then_with(|| left.micro_node_id.cmp(&right.micro_node_id))
    });

    let mut ids = BTreeSet::new();
    for candidate in &report.candidates {
        if !ids.insert(candidate.micro_node_id.clone()) {
            report.duplicate_candidate_count += 1;
        }
        if !mvp4_source_span_is_bounded(&candidate.source_span) {
            report.source_span_failures += 1;
        }
        if candidate.node_kind == MicroNodeKind::CallSite && candidate.symbol_binding_id.is_some() {
            report.callee_target_overclaim_count += 1;
        }
        if candidate.node_kind == MicroNodeKind::AssignmentSite
            && candidate.symbol_binding_id.is_some()
        {
            report.write_flow_overclaim_count += 1;
        }
    }
    report
}

pub fn emit_mvp4_typescript_value_use_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptValueUseCandidateReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    emit_mvp4_typescript_value_use_candidates_with_scopes(parsed, source, &scope_report)
}

fn emit_mvp4_typescript_value_use_candidates_with_scopes(
    parsed: &ParsedFile,
    source: &str,
    scope_report: &Mvp4TypeScriptScopeDiscoveryReport,
) -> Mvp4TypeScriptValueUseCandidateReport {
    let mut report = Mvp4TypeScriptValueUseCandidateReport {
        status: scope_report.status.clone(),
        eligible: scope_report.eligible,
        exclusion_reason: scope_report.exclusion_reason.clone(),
        source_role: scope_report.source_role,
        parser_status: scope_report.parser_status.clone(),
        candidates: Vec::new(),
        duplicate_candidate_count: 0,
        source_span_failures: 0,
        binding_overclaim_count: 0,
        production_persistence_enabled: false,
        production_micro_node_rows: 0,
        micro_edge_rows: 0,
        flow_proof_activated: false,
    };

    if !scope_report.eligible {
        return report;
    }

    let root = parsed.tree().root_node();
    let function_nodes_by_span =
        index_mvp4_ts_function_nodes_by_span(root, &parsed.repo_relative_path);
    let mut occurrence_counts = BTreeMap::<(MicroNodeKind, Vec<String>, String), u32>::new();
    for function in &scope_report.functions {
        let Some(function_node) = function_nodes_by_span
            .get(&mvp4_source_span_key(&function.source_span))
            .copied()
        else {
            continue;
        };
        let Some(body_node) = function_node.child_by_field_name("body") else {
            continue;
        };
        collect_mvp4_ts_value_use_candidates(
            body_node,
            parsed,
            source,
            function,
            &mut occurrence_counts,
            &mut report.candidates,
        );
    }

    report.candidates.sort_by(|left, right| {
        left.source_span
            .start_line
            .cmp(&right.source_span.start_line)
            .then_with(|| {
                left.source_span
                    .start_column
                    .cmp(&right.source_span.start_column)
            })
            .then_with(|| left.node_kind.cmp(&right.node_kind))
            .then_with(|| left.name_or_literal.cmp(&right.name_or_literal))
            .then_with(|| left.micro_node_id.cmp(&right.micro_node_id))
    });

    let mut ids = BTreeSet::new();
    for candidate in &report.candidates {
        if !ids.insert(candidate.micro_node_id.clone()) {
            report.duplicate_candidate_count += 1;
        }
        if !mvp4_source_span_is_bounded(&candidate.source_span) {
            report.source_span_failures += 1;
        }
        if candidate.symbol_binding_id.is_some() {
            report.binding_overclaim_count += 1;
        }
    }
    report
}

/// True when `node` is a bare `identifier` in value-read position inside a
/// function body. Conservative: excludes declaration names, assignment/`for..of`
/// write targets, the direct callee of a call, and update-expression operands.
/// Property names and shorthand keys are other AST kinds and are excluded by the
/// `identifier`-only gate.
fn is_mvp4_ts_value_use_identifier(node: Node<'_>) -> bool {
    if node.kind() != "identifier" {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let node_id = node.id();
    let field_is = |field: &str| parent.child_by_field_name(field).map(|n| n.id()) == Some(node_id);
    match parent.kind() {
        // Declaration names are LocalBinding/Parameter declarations, not reads.
        "variable_declarator" | "required_parameter" | "optional_parameter" => !field_is("name"),
        // Assignment LHS is a write target (AssignmentSite owns it), not a read.
        "assignment_expression" | "augmented_assignment_expression" => !field_is("left"),
        // The callee identifier belongs to the CallSite, not a value read.
        "call_expression" => !field_is("function"),
        // `for (x of/in ...)` binding target is a write, not a read.
        "for_in_statement" => !field_is("left"),
        // `x++` / `--x` read+write a binding; defer to a future MutationSite.
        "update_expression" => false,
        _ => true,
    }
}

fn collect_mvp4_ts_value_use_candidates(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    function: &Mvp4TypeScriptFunctionScope,
    occurrence_counts: &mut BTreeMap<(MicroNodeKind, Vec<String>, String), u32>,
    candidates: &mut Vec<Mvp4TypeScriptMicroNodeCandidate>,
) {
    if node.is_error() || node.is_missing() || is_mvp4_ts_function_scope_node(node) {
        return;
    }

    if is_mvp4_ts_value_use_identifier(node) {
        let source_span = source_span_for_node(&parsed.repo_relative_path, node);
        let parse_recovery = function.parse_recovery || node_has_error_or_missing_descendant(node);
        let mut scope_path = function.scope_path.clone();
        scope_path.push("value_uses".to_string());
        let name_or_literal = mvp4_ts_value_use_label(node, source);
        let structural_ast_path = mvp4_ts_value_use_structural_path(function, &name_or_literal);
        push_mvp4_ts_candidate(
            parsed,
            function,
            MicroNodeKind::ValueUse,
            MVP4_TS_VALUE_USE_EXTRACTOR,
            &scope_path,
            &structural_ast_path,
            &source_span,
            &name_or_literal,
            // Existence-only: binding identity is unresolved until the B2 resolver.
            None,
            Some(function.function_kind.clone()),
            None,
            parse_recovery,
            occurrence_counts,
            candidates,
        );
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_mvp4_ts_function_scope_node(child) {
            continue;
        }
        collect_mvp4_ts_value_use_candidates(
            child,
            parsed,
            source,
            function,
            occurrence_counts,
            candidates,
        );
    }
}

fn mvp4_ts_value_use_structural_path(
    function: &Mvp4TypeScriptFunctionScope,
    name_or_literal: &str,
) -> Vec<String> {
    function
        .structural_ast_path
        .iter()
        .cloned()
        .chain([
            "value_use".to_string(),
            format!("syntax:{}", compact_identity_hash(name_or_literal)),
        ])
        .collect()
}

fn mvp4_ts_value_use_label(node: Node<'_>, source: &str) -> String {
    node_text(node, source)
        .map(|text| compact_extracted_label(&text, node))
        .unwrap_or_else(|| format!("value_use@{}", node.start_byte()))
}

/// Capped, production, exact `ValueUse` micro-node candidates for persistence
/// (MVP4.2b). These are the head endpoints required by persisted `LOCAL_READS`
/// edges. Existence-only (no resolved binding); parse-recovery candidates are
/// not persisted. Kept separate from the frozen MVP4.1 first-slice inventory.
pub fn emit_mvp4_2b_typescript_value_use_persistable_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Vec<Mvp4TypeScriptMicroNodeCandidate> {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    let report =
        emit_mvp4_typescript_value_use_candidates_with_scopes(parsed, source, &scope_report);
    mvp4_2b_persistable_value_use_candidates_from_report(&report)
}

fn mvp4_2b_persistable_value_use_candidates_from_report(
    report: &Mvp4TypeScriptValueUseCandidateReport,
) -> Vec<Mvp4TypeScriptMicroNodeCandidate> {
    if !report.eligible {
        return Vec::new();
    }
    let mut per_function: BTreeMap<String, usize> = BTreeMap::new();
    let mut kept = Vec::new();
    for candidate in &report.candidates {
        if candidate.parse_recovery {
            continue;
        }
        let count = per_function
            .entry(candidate.enclosing_function_identity.clone())
            .or_insert(0);
        if *count >= MVP4_TS_MAX_VALUE_USE_PER_FUNCTION {
            continue;
        }
        *count += 1;
        kept.push(candidate.clone());
    }
    kept
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mvp4TypeScriptBindingResolutionStatus {
    Resolved,
    Unresolved,
}

/// Result of MVP4.2b local binding resolution for a single read/write
/// occurrence. `Resolved` carries the proven declaring binding; `Unresolved`
/// carries a reason and never a binding id (no name-only guessing — the proof
/// model forbids guessing a binding by name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptBindingResolution {
    pub status: Mvp4TypeScriptBindingResolutionStatus,
    pub symbol_binding_id: Option<String>,
    pub binding_kind: Option<String>,
    pub binding_declaration_kind: Option<String>,
    pub binding_source_span: Option<SourceSpan>,
    pub binding_scope_path: Option<Vec<String>>,
    pub unresolved_reason: Option<String>,
}

fn mvp4_source_span_contains(outer: &SourceSpan, inner: &SourceSpan) -> bool {
    let starts_at_or_before =
        (outer.start_line, outer.start_column) <= (inner.start_line, inner.start_column);
    let ends_at_or_after = (outer.end_line, outer.end_column) >= (inner.end_line, inner.end_column);
    starts_at_or_before && ends_at_or_after
}

/// Resolve a function-local read/write occurrence (`name` at `occurrence_span`)
/// to its declaring `Parameter`/`LocalBinding` using lexical scope containment
/// with correct shadowing: the binding in the innermost enclosing scope that
/// declares `name` at or before the use. Parameters are visible across the whole
/// function; locals must be declared before the use. Returns `Unresolved` for
/// free variables, imports, globals, and recovery — never a guessed binding.
pub fn resolve_mvp4_ts_local_binding(
    function: &Mvp4TypeScriptFunctionScope,
    name: &str,
    occurrence_span: &SourceSpan,
) -> Mvp4TypeScriptBindingResolution {
    let unresolved = |reason: &str| Mvp4TypeScriptBindingResolution {
        status: Mvp4TypeScriptBindingResolutionStatus::Unresolved,
        symbol_binding_id: None,
        binding_kind: None,
        binding_declaration_kind: None,
        binding_source_span: None,
        binding_scope_path: None,
        unresolved_reason: Some(reason.to_string()),
    };

    if function.parse_recovery {
        return unresolved("parse_recovery");
    }

    // Innermost enclosing scope (by effective-span start) that declares `name`.
    let mut best: Option<((u32, Option<u32>), &Mvp4TypeScriptScopeBinding)> = None;
    let scopes = std::iter::once(&function.parameter_scope).chain(function.lexical_scopes.iter());
    for scope in scopes {
        let is_parameter_scope = std::ptr::eq(scope, &function.parameter_scope);
        // Parameters are in scope across the whole function, not just the `(...)`
        // list span; body/block scopes use their own span.
        let scope_span = if is_parameter_scope {
            &function.source_span
        } else {
            &scope.source_span
        };
        if !mvp4_source_span_contains(scope_span, occurrence_span) {
            continue;
        }

        let mut scope_binding: Option<&Mvp4TypeScriptScopeBinding> = None;
        for binding in &scope.bindings {
            if binding.name != name {
                continue;
            }
            if !is_parameter_scope {
                let declared_at_or_before =
                    (
                        binding.source_span.start_line,
                        binding.source_span.start_column,
                    ) <= (occurrence_span.start_line, occurrence_span.start_column);
                if !declared_at_or_before {
                    continue;
                }
            }
            // Latest same-name declaration within this scope.
            scope_binding = match scope_binding {
                Some(current)
                    if (
                        current.source_span.start_line,
                        current.source_span.start_column,
                    ) >= (
                        binding.source_span.start_line,
                        binding.source_span.start_column,
                    ) =>
                {
                    Some(current)
                }
                _ => Some(binding),
            };
        }

        if let Some(binding) = scope_binding {
            let scope_start = (scope_span.start_line, scope_span.start_column);
            best = match best {
                Some((best_start, best_binding)) if best_start >= scope_start => {
                    Some((best_start, best_binding))
                }
                _ => Some((scope_start, binding)),
            };
        }
    }

    let Some((_, binding)) = best else {
        return unresolved("no_enclosing_declaration");
    };

    let span_key = format!(
        "{}:{}:{}:{}",
        binding.source_span.start_line,
        binding.source_span.start_column.unwrap_or_default(),
        binding.source_span.end_line,
        binding.source_span.end_column.unwrap_or_default()
    );
    let scope_key = binding.scope_path.join("/");
    let symbol_binding_id = stable_fact_identity_key(
        "mvp4_ts_local_binding_resolution",
        [
            function.function_identity.as_str(),
            scope_key.as_str(),
            name,
            span_key.as_str(),
        ],
    );

    Mvp4TypeScriptBindingResolution {
        status: Mvp4TypeScriptBindingResolutionStatus::Resolved,
        symbol_binding_id: Some(symbol_binding_id),
        binding_kind: Some(binding.binding_kind.clone()),
        binding_declaration_kind: binding.declaration_kind.clone(),
        binding_source_span: Some(binding.source_span.clone()),
        binding_scope_path: Some(binding.scope_path.clone()),
        unresolved_reason: None,
    }
}

/// MVP4.2b read resolution report: every `ValueUse` read with its resolved
/// `symbol_binding_id` attached when the local binding resolver proves the
/// declaration, plus resolution statistics. Unresolved reads keep
/// `symbol_binding_id = None` and are tallied by reason. This is the read
/// endpoint feed for the future exact `LOCAL_READS` edge (B3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptResolvedValueUseReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<Mvp4TypeScriptMicroNodeCandidate>,
    pub resolved_count: u64,
    pub unresolved_count: u64,
    pub unresolved_by_reason: BTreeMap<String, u64>,
    pub production_persistence_enabled: bool,
    pub production_micro_node_rows: u64,
    pub micro_edge_rows: u64,
    pub flow_proof_activated: bool,
}

pub fn emit_mvp4_typescript_resolved_value_use_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptResolvedValueUseReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    let uses = emit_mvp4_typescript_value_use_candidates(parsed, source);
    let mut report = Mvp4TypeScriptResolvedValueUseReport {
        status: scope_report.status.clone(),
        eligible: scope_report.eligible,
        exclusion_reason: scope_report.exclusion_reason.clone(),
        source_role: scope_report.source_role,
        parser_status: scope_report.parser_status.clone(),
        candidates: Vec::new(),
        resolved_count: 0,
        unresolved_count: 0,
        unresolved_by_reason: BTreeMap::new(),
        production_persistence_enabled: false,
        production_micro_node_rows: 0,
        micro_edge_rows: 0,
        flow_proof_activated: false,
    };

    if !scope_report.eligible {
        return report;
    }

    let functions: BTreeMap<&str, &Mvp4TypeScriptFunctionScope> = scope_report
        .functions
        .iter()
        .map(|function| (function.function_identity.as_str(), function))
        .collect();

    for mut candidate in uses.candidates {
        let resolution = match functions.get(candidate.enclosing_function_identity.as_str()) {
            Some(function) => resolve_mvp4_ts_local_binding(
                function,
                &candidate.name_or_literal,
                &candidate.source_span,
            ),
            None => Mvp4TypeScriptBindingResolution {
                status: Mvp4TypeScriptBindingResolutionStatus::Unresolved,
                symbol_binding_id: None,
                binding_kind: None,
                binding_declaration_kind: None,
                binding_source_span: None,
                binding_scope_path: None,
                unresolved_reason: Some("function_scope_missing".to_string()),
            },
        };
        match resolution.status {
            Mvp4TypeScriptBindingResolutionStatus::Resolved => {
                candidate.symbol_binding_id = resolution.symbol_binding_id;
                report.resolved_count += 1;
            }
            Mvp4TypeScriptBindingResolutionStatus::Unresolved => {
                let reason = resolution
                    .unresolved_reason
                    .unwrap_or_else(|| "unknown".to_string());
                *report.unresolved_by_reason.entry(reason).or_insert(0) += 1;
                report.unresolved_count += 1;
            }
        }
        report.candidates.push(candidate);
    }
    report
}

fn mvp4_source_span_key(span: &SourceSpan) -> (u32, Option<u32>, u32, Option<u32>) {
    (
        span.start_line,
        span.start_column,
        span.end_line,
        span.end_column,
    )
}

/// MVP4.2b in-memory `LOCAL_READS` edge report: exact edges from a `ValueUse`
/// read to the resolver-proven declaring `Parameter`/`LocalBinding` micro-node.
/// Edges are emitted only when the read resolves to a retained declaration
/// micro-node; unresolved reads (free vars/imports) and cap-omitted declarations
/// produce no edge and are tallied, never faked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptLocalReadsInMemoryReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<MicroEdgeCandidate>,
    pub resolved_read_count: u64,
    pub unresolved_read_count: u64,
    pub omitted_edge_count: u64,
    pub duplicate_candidate_count: u64,
    pub claimable_edge_missing_span_count: u64,
    pub claimable_edge_missing_provenance_count: u64,
    pub flow_semantic_overclaim_count: u64,
    pub production_micro_edge_rows: u64,
    pub mutation_proof_activated: bool,
    pub flow_proof_activated: bool,
}

pub fn emit_mvp4_typescript_local_reads_in_memory_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptLocalReadsInMemoryReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    let policy = mvp4_typescript_first_slice_cap_policy();
    let inventory = emit_mvp4_typescript_in_memory_first_slice_inventory_with_scopes_and_policy(
        parsed,
        source,
        &policy,
        &scope_report,
    );
    let value_uses =
        emit_mvp4_typescript_value_use_candidates_with_scopes(parsed, source, &scope_report);
    emit_mvp4_typescript_local_reads_in_memory_candidates_with_context(
        &scope_report,
        &inventory,
        &value_uses,
    )
}

fn emit_mvp4_typescript_local_reads_in_memory_candidates_with_context(
    scope_report: &Mvp4TypeScriptScopeDiscoveryReport,
    inventory: &Mvp4TypeScriptInMemoryInventoryReport,
    value_uses: &Mvp4TypeScriptValueUseCandidateReport,
) -> Mvp4TypeScriptLocalReadsInMemoryReport {
    let mut report = Mvp4TypeScriptLocalReadsInMemoryReport {
        status: scope_report.status.clone(),
        eligible: scope_report.eligible,
        exclusion_reason: scope_report.exclusion_reason.clone(),
        source_role: scope_report.source_role,
        parser_status: scope_report.parser_status.clone(),
        candidates: Vec::new(),
        resolved_read_count: 0,
        unresolved_read_count: 0,
        omitted_edge_count: 0,
        duplicate_candidate_count: 0,
        claimable_edge_missing_span_count: 0,
        claimable_edge_missing_provenance_count: 0,
        flow_semantic_overclaim_count: 0,
        production_micro_edge_rows: 0,
        mutation_proof_activated: false,
        flow_proof_activated: false,
    };

    if !scope_report.eligible {
        return report;
    }

    // Retained declaration micro-nodes keyed by (name, span) — the same
    // `binding.source_span` the resolver returns, so the match is exact.
    type SpanKey = (u32, Option<u32>, u32, Option<u32>);
    let declaration_index: BTreeMap<(String, SpanKey), &Mvp4TypeScriptMicroNodeCandidate> =
        inventory
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.node_kind,
                    MicroNodeKind::Parameter | MicroNodeKind::LocalBinding
                )
            })
            .map(|candidate| {
                (
                    (
                        candidate.name_or_literal.clone(),
                        mvp4_source_span_key(&candidate.source_span),
                    ),
                    candidate,
                )
            })
            .collect();

    let functions: BTreeMap<&str, &Mvp4TypeScriptFunctionScope> = scope_report
        .functions
        .iter()
        .map(|function| (function.function_identity.as_str(), function))
        .collect();

    let mut seen_edge_ids = BTreeSet::new();
    for read in &value_uses.candidates {
        let Some(function) = functions.get(read.enclosing_function_identity.as_str()) else {
            continue;
        };
        let resolution =
            resolve_mvp4_ts_local_binding(function, &read.name_or_literal, &read.source_span);
        let Mvp4TypeScriptBindingResolution {
            status: Mvp4TypeScriptBindingResolutionStatus::Resolved,
            binding_source_span: Some(binding_span),
            ..
        } = resolution
        else {
            report.unresolved_read_count += 1;
            continue;
        };
        report.resolved_read_count += 1;

        let key = (
            read.name_or_literal.clone(),
            mvp4_source_span_key(&binding_span),
        );
        let Some(declaration) = declaration_index.get(&key) else {
            // Resolved to a binding whose declaration micro-node was cap-omitted.
            report.omitted_edge_count += 1;
            continue;
        };

        if read.parse_recovery || declaration.parse_recovery {
            continue;
        }
        if !mvp4_source_span_is_bounded(&read.source_span)
            || !mvp4_source_span_is_bounded(&declaration.source_span)
        {
            report.claimable_edge_missing_span_count += 1;
            continue;
        }

        let provenance = MicroFactProvenance {
            derivation_kind: MicroDerivationKind::DirectAstExtraction,
            source_fact_ids: vec![
                read.micro_node_id.clone(),
                declaration.micro_node_id.clone(),
            ],
            source_spans: vec![read.source_span.clone(), declaration.source_span.clone()],
            extractor_or_adapter_version: MVP4_2B_LOCAL_READS_EXTRACTION_VERSION.to_string(),
            exactness: MicroExactness::Exact,
            limitations: vec![
                "local_resolver_proven_read_occurrence_only".to_string(),
                "does_not_prove_value_flow".to_string(),
                "does_not_prove_mutation".to_string(),
                "does_not_prove_runtime_value".to_string(),
                "does_not_generate_local_flow_packet".to_string(),
                "does_not_activate_mutation_proof_or_flow_proof".to_string(),
            ],
        };
        if validate_micro_fact_provenance(&provenance).is_err() {
            report.claimable_edge_missing_provenance_count += 1;
            continue;
        }

        let identity = MicroEdgeIdentityInput {
            repo_relative_path: read.repo_relative_path.clone(),
            language: format!("{}@{}", read.language, read.frontend),
            kind: MicroEdgeKind::LocalReads,
            function_entity_id: Some(read.enclosing_function_identity.clone()),
            scope_path: read.scope_path.clone(),
            source_micro_node_id: read.micro_node_id.clone(),
            target_micro_node_id: declaration.micro_node_id.clone(),
            structural_path: vec![
                "relation:local_reads".to_string(),
                format!("frontend:{}", read.frontend),
                format!("head_kind:{}", MicroNodeKind::ValueUse.as_str()),
                format!("tail_kind:{}", declaration.node_kind.as_str()),
                format!("read_node:{}", read.micro_node_id),
            ],
            occurrence_index: read.occurrence_index,
            source_span: Some(read.source_span.clone()),
        };
        let micro_edge_id = stable_micro_edge_id(&identity);
        if !seen_edge_ids.insert(micro_edge_id.clone()) {
            report.duplicate_candidate_count += 1;
            continue;
        }

        report.candidates.push(MicroEdgeCandidate {
            micro_edge_id,
            micro_edge_kind: MicroEdgeKind::LocalReads,
            head_micro_node_id: read.micro_node_id.clone(),
            tail_micro_node_id: declaration.micro_node_id.clone(),
            repo_relative_path: read.repo_relative_path.clone(),
            function_identity: read.enclosing_function_identity.clone(),
            relation_source_span: read.source_span.clone(),
            head_source_span: read.source_span.clone(),
            tail_source_span: declaration.source_span.clone(),
            provenance,
            exactness: MicroExactness::Exact,
            claimability: MVP4_2B_LOCAL_READS_CLAIMABILITY.to_string(),
            language: read.language.clone(),
            frontend: read.frontend.clone(),
            source_role: read.source_role,
            row_schema_version: MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION,
            payload_version: MVP4_2_MICRO_EDGE_PAYLOAD_VERSION,
            extraction_version: MVP4_2B_LOCAL_READS_EXTRACTION_VERSION.to_string(),
            cap_omission_state: None,
        });
    }

    sort_mvp4_micro_edge_candidates(&mut report.candidates);
    // Integrity: LOCAL_READS never claims value flow or mutation.
    report.flow_semantic_overclaim_count = report
        .candidates
        .iter()
        .filter(|candidate| {
            !candidate
                .provenance
                .limitations
                .iter()
                .any(|limitation| limitation == "does_not_prove_value_flow")
        })
        .count() as u64;
    report
}

#[derive(Debug, Clone)]
struct Mvp4TsWriteTarget {
    name: String,
    lhs_span: SourceSpan,
    assignment_span: SourceSpan,
    enclosing_function_identity: String,
    parse_recovery: bool,
}

fn collect_mvp4_ts_write_targets(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    function: &Mvp4TypeScriptFunctionScope,
    targets: &mut Vec<Mvp4TsWriteTarget>,
) {
    if node.is_error() || node.is_missing() || is_mvp4_ts_function_scope_node(node) {
        return;
    }
    if matches!(
        node.kind(),
        "assignment_expression" | "augmented_assignment_expression"
    ) {
        if let Some(left) = node.child_by_field_name("left") {
            // Simple identifier write targets only; member/index/destructuring
            // write targets stay unsupported (no exact edge) for this slice.
            if left.kind() == "identifier" {
                if let Some(name) = node_text(left, source) {
                    targets.push(Mvp4TsWriteTarget {
                        name,
                        lhs_span: source_span_for_node(&parsed.repo_relative_path, left),
                        assignment_span: source_span_for_node(&parsed.repo_relative_path, node),
                        enclosing_function_identity: function.function_identity.clone(),
                        parse_recovery: function.parse_recovery
                            || node_has_error_or_missing_descendant(node),
                    });
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_mvp4_ts_function_scope_node(child) {
            continue;
        }
        collect_mvp4_ts_write_targets(child, parsed, source, function, targets);
    }
}

/// MVP4.2b in-memory `LOCAL_WRITES` edge report: exact edges from an
/// `AssignmentSite` to the resolver-proven declaring `Parameter`/`LocalBinding`
/// of its simple-identifier LHS. Unresolved targets (globals/imports) and
/// member/index/destructuring LHS produce no edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptLocalWritesInMemoryReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<MicroEdgeCandidate>,
    pub resolved_write_count: u64,
    pub unresolved_write_count: u64,
    pub omitted_edge_count: u64,
    pub duplicate_candidate_count: u64,
    pub claimable_edge_missing_span_count: u64,
    pub claimable_edge_missing_provenance_count: u64,
    pub flow_semantic_overclaim_count: u64,
    pub production_micro_edge_rows: u64,
    pub mutation_proof_activated: bool,
    pub flow_proof_activated: bool,
}

pub fn emit_mvp4_typescript_local_writes_in_memory_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptLocalWritesInMemoryReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    let policy = mvp4_typescript_first_slice_cap_policy();
    let inventory = emit_mvp4_typescript_in_memory_first_slice_inventory_with_scopes_and_policy(
        parsed,
        source,
        &policy,
        &scope_report,
    );
    emit_mvp4_typescript_local_writes_in_memory_candidates_with_context(
        parsed,
        source,
        &scope_report,
        &inventory,
    )
}

fn emit_mvp4_typescript_local_writes_in_memory_candidates_with_context(
    parsed: &ParsedFile,
    source: &str,
    scope_report: &Mvp4TypeScriptScopeDiscoveryReport,
    inventory: &Mvp4TypeScriptInMemoryInventoryReport,
) -> Mvp4TypeScriptLocalWritesInMemoryReport {
    let mut report = Mvp4TypeScriptLocalWritesInMemoryReport {
        status: scope_report.status.clone(),
        eligible: scope_report.eligible,
        exclusion_reason: scope_report.exclusion_reason.clone(),
        source_role: scope_report.source_role,
        parser_status: scope_report.parser_status.clone(),
        candidates: Vec::new(),
        resolved_write_count: 0,
        unresolved_write_count: 0,
        omitted_edge_count: 0,
        duplicate_candidate_count: 0,
        claimable_edge_missing_span_count: 0,
        claimable_edge_missing_provenance_count: 0,
        flow_semantic_overclaim_count: 0,
        production_micro_edge_rows: 0,
        mutation_proof_activated: false,
        flow_proof_activated: false,
    };

    if !scope_report.eligible {
        return report;
    }

    type SpanKey = (u32, Option<u32>, u32, Option<u32>);
    let declaration_index: BTreeMap<(String, SpanKey), &Mvp4TypeScriptMicroNodeCandidate> =
        inventory
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.node_kind,
                    MicroNodeKind::Parameter | MicroNodeKind::LocalBinding
                )
            })
            .map(|candidate| {
                (
                    (
                        candidate.name_or_literal.clone(),
                        mvp4_source_span_key(&candidate.source_span),
                    ),
                    candidate,
                )
            })
            .collect();
    let assignment_index: BTreeMap<SpanKey, &Mvp4TypeScriptMicroNodeCandidate> = inventory
        .candidates
        .iter()
        .filter(|candidate| candidate.node_kind == MicroNodeKind::AssignmentSite)
        .map(|candidate| (mvp4_source_span_key(&candidate.source_span), candidate))
        .collect();
    let functions: BTreeMap<&str, &Mvp4TypeScriptFunctionScope> = scope_report
        .functions
        .iter()
        .map(|function| (function.function_identity.as_str(), function))
        .collect();

    let root = parsed.tree().root_node();
    let function_nodes_by_span =
        index_mvp4_ts_function_nodes_by_span(root, &parsed.repo_relative_path);
    let mut targets = Vec::new();
    for function in &scope_report.functions {
        let Some(function_node) = function_nodes_by_span
            .get(&mvp4_source_span_key(&function.source_span))
            .copied()
        else {
            continue;
        };
        let Some(body_node) = function_node.child_by_field_name("body") else {
            continue;
        };
        collect_mvp4_ts_write_targets(body_node, parsed, source, function, &mut targets);
    }

    let mut seen_edge_ids = BTreeSet::new();
    for target in &targets {
        let Some(function) = functions.get(target.enclosing_function_identity.as_str()) else {
            continue;
        };
        let resolution = resolve_mvp4_ts_local_binding(function, &target.name, &target.lhs_span);
        let Mvp4TypeScriptBindingResolution {
            status: Mvp4TypeScriptBindingResolutionStatus::Resolved,
            binding_source_span: Some(binding_span),
            ..
        } = resolution
        else {
            report.unresolved_write_count += 1;
            continue;
        };
        report.resolved_write_count += 1;

        let decl_key = (target.name.clone(), mvp4_source_span_key(&binding_span));
        let Some(declaration) = declaration_index.get(&decl_key) else {
            report.omitted_edge_count += 1;
            continue;
        };
        let Some(assignment_site) =
            assignment_index.get(&mvp4_source_span_key(&target.assignment_span))
        else {
            report.omitted_edge_count += 1;
            continue;
        };

        if target.parse_recovery || declaration.parse_recovery || assignment_site.parse_recovery {
            continue;
        }
        if !mvp4_source_span_is_bounded(&assignment_site.source_span)
            || !mvp4_source_span_is_bounded(&declaration.source_span)
        {
            report.claimable_edge_missing_span_count += 1;
            continue;
        }

        let provenance = MicroFactProvenance {
            derivation_kind: MicroDerivationKind::DirectAstExtraction,
            source_fact_ids: vec![
                assignment_site.micro_node_id.clone(),
                declaration.micro_node_id.clone(),
            ],
            source_spans: vec![target.lhs_span.clone(), declaration.source_span.clone()],
            extractor_or_adapter_version: MVP4_2B_LOCAL_WRITES_EXTRACTION_VERSION.to_string(),
            exactness: MicroExactness::Exact,
            limitations: vec![
                "local_resolver_proven_write_target_only".to_string(),
                "does_not_prove_value_flow".to_string(),
                "does_not_prove_mutation_semantics".to_string(),
                "does_not_prove_runtime_value".to_string(),
                "does_not_generate_local_flow_packet".to_string(),
                "does_not_activate_mutation_proof_or_flow_proof".to_string(),
            ],
        };
        if validate_micro_fact_provenance(&provenance).is_err() {
            report.claimable_edge_missing_provenance_count += 1;
            continue;
        }

        let identity = MicroEdgeIdentityInput {
            repo_relative_path: assignment_site.repo_relative_path.clone(),
            language: format!("{}@{}", assignment_site.language, assignment_site.frontend),
            kind: MicroEdgeKind::LocalWrites,
            function_entity_id: Some(assignment_site.enclosing_function_identity.clone()),
            scope_path: assignment_site.scope_path.clone(),
            source_micro_node_id: assignment_site.micro_node_id.clone(),
            target_micro_node_id: declaration.micro_node_id.clone(),
            structural_path: vec![
                "relation:local_writes".to_string(),
                format!("frontend:{}", assignment_site.frontend),
                format!("head_kind:{}", MicroNodeKind::AssignmentSite.as_str()),
                format!("tail_kind:{}", declaration.node_kind.as_str()),
                format!("write_node:{}", assignment_site.micro_node_id),
            ],
            occurrence_index: assignment_site.occurrence_index,
            source_span: Some(target.lhs_span.clone()),
        };
        let micro_edge_id = stable_micro_edge_id(&identity);
        if !seen_edge_ids.insert(micro_edge_id.clone()) {
            report.duplicate_candidate_count += 1;
            continue;
        }

        report.candidates.push(MicroEdgeCandidate {
            micro_edge_id,
            micro_edge_kind: MicroEdgeKind::LocalWrites,
            head_micro_node_id: assignment_site.micro_node_id.clone(),
            tail_micro_node_id: declaration.micro_node_id.clone(),
            repo_relative_path: assignment_site.repo_relative_path.clone(),
            function_identity: assignment_site.enclosing_function_identity.clone(),
            relation_source_span: target.lhs_span.clone(),
            head_source_span: assignment_site.source_span.clone(),
            tail_source_span: declaration.source_span.clone(),
            provenance,
            exactness: MicroExactness::Exact,
            claimability: MVP4_2B_LOCAL_WRITES_CLAIMABILITY.to_string(),
            language: assignment_site.language.clone(),
            frontend: assignment_site.frontend.clone(),
            source_role: assignment_site.source_role,
            row_schema_version: MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION,
            payload_version: MVP4_2_MICRO_EDGE_PAYLOAD_VERSION,
            extraction_version: MVP4_2B_LOCAL_WRITES_EXTRACTION_VERSION.to_string(),
            cap_omission_state: None,
        });
    }

    sort_mvp4_micro_edge_candidates(&mut report.candidates);
    // Integrity: LOCAL_WRITES never claims value flow or mutation semantics.
    report.flow_semantic_overclaim_count = report
        .candidates
        .iter()
        .filter(|candidate| {
            !candidate
                .provenance
                .limitations
                .iter()
                .any(|limitation| limitation == "does_not_prove_value_flow")
        })
        .count() as u64;
    report
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mvp4TsFlowTargetKind {
    Assignment,
    Initializer,
}

impl Mvp4TsFlowTargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Assignment => "assignment",
            Self::Initializer => "initializer",
        }
    }
}

#[derive(Debug, Clone)]
struct Mvp4TsFlowTarget {
    kind: Mvp4TsFlowTargetKind,
    name: String,
    lhs_span: SourceSpan,
    relation_span: SourceSpan,
    value_span: SourceSpan,
    enclosing_function_identity: String,
    parse_recovery: bool,
}

fn collect_mvp4_ts_local_flow_targets(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    function: &Mvp4TypeScriptFunctionScope,
    targets: &mut Vec<Mvp4TsFlowTarget>,
) {
    if node.is_error() || node.is_missing() || is_mvp4_ts_function_scope_node(node) {
        return;
    }

    if node.kind() == "assignment_expression" {
        if let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) {
            if left.kind() == "identifier" {
                if let Some(name) = node_text(left, source) {
                    targets.push(Mvp4TsFlowTarget {
                        kind: Mvp4TsFlowTargetKind::Assignment,
                        name,
                        lhs_span: source_span_for_node(&parsed.repo_relative_path, left),
                        relation_span: source_span_for_node(&parsed.repo_relative_path, node),
                        value_span: source_span_for_node(&parsed.repo_relative_path, right),
                        enclosing_function_identity: function.function_identity.clone(),
                        parse_recovery: function.parse_recovery
                            || node_has_error_or_missing_descendant(node),
                    });
                }
            }
        }
    } else if node.kind() == "variable_declarator" {
        if let (Some(name_node), Some(value_node)) = (
            node.child_by_field_name("name"),
            variable_declarator_value_node(node),
        ) {
            if name_node.kind() == "identifier" {
                if let Some(name) = node_text(name_node, source) {
                    targets.push(Mvp4TsFlowTarget {
                        kind: Mvp4TsFlowTargetKind::Initializer,
                        name,
                        lhs_span: source_span_for_node(&parsed.repo_relative_path, name_node),
                        relation_span: source_span_for_node(&parsed.repo_relative_path, node),
                        value_span: source_span_for_node(&parsed.repo_relative_path, value_node),
                        enclosing_function_identity: function.function_identity.clone(),
                        parse_recovery: function.parse_recovery
                            || node_has_error_or_missing_descendant(node),
                    });
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_mvp4_ts_function_scope_node(child) {
            continue;
        }
        collect_mvp4_ts_local_flow_targets(child, parsed, source, function, targets);
    }
}

/// MVP4.2b in-memory `LOCAL_FLOWS_TO` edge report: derived, function-local
/// assignment-chain candidates from resolver-proven source bindings to a
/// resolver-proven target binding. This is deliberately NOT wired into the
/// production micro-edge wrapper yet; no rows, packets, `flow_proof`, or
/// `mutation_proof` are activated by this parser-only B4b gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mvp4TypeScriptLocalFlowsToInMemoryReport {
    pub status: String,
    pub eligible: bool,
    pub exclusion_reason: Option<String>,
    pub source_role: MicroSourceRole,
    pub parser_status: String,
    pub candidates: Vec<MicroEdgeCandidate>,
    pub assignment_flow_count: u64,
    pub initializer_flow_count: u64,
    pub unresolved_read_count: u64,
    pub unresolved_target_count: u64,
    pub omitted_edge_count: u64,
    pub duplicate_candidate_count: u64,
    pub claimable_edge_missing_span_count: u64,
    pub claimable_edge_missing_provenance_count: u64,
    pub exactness_overclaim_count: u64,
    pub production_micro_edge_rows: u64,
    pub local_flow_packet_rows: u64,
    pub mutation_proof_activated: bool,
    pub flow_proof_activated: bool,
}

pub fn emit_mvp4_typescript_local_flows_to_in_memory_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptLocalFlowsToInMemoryReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    let policy = mvp4_typescript_first_slice_cap_policy();
    let inventory = emit_mvp4_typescript_in_memory_first_slice_inventory_with_scopes_and_policy(
        parsed,
        source,
        &policy,
        &scope_report,
    );
    let value_uses =
        emit_mvp4_typescript_value_use_candidates_with_scopes(parsed, source, &scope_report);
    let reads = emit_mvp4_typescript_local_reads_in_memory_candidates_with_context(
        &scope_report,
        &inventory,
        &value_uses,
    );
    let writes = emit_mvp4_typescript_local_writes_in_memory_candidates_with_context(
        parsed,
        source,
        &scope_report,
        &inventory,
    );
    emit_mvp4_typescript_local_flows_to_in_memory_candidates_with_context(
        parsed,
        source,
        &scope_report,
        &inventory,
        &reads,
        &writes,
    )
}

fn emit_mvp4_typescript_local_flows_to_in_memory_candidates_with_context(
    parsed: &ParsedFile,
    source: &str,
    scope_report: &Mvp4TypeScriptScopeDiscoveryReport,
    inventory: &Mvp4TypeScriptInMemoryInventoryReport,
    reads: &Mvp4TypeScriptLocalReadsInMemoryReport,
    writes: &Mvp4TypeScriptLocalWritesInMemoryReport,
) -> Mvp4TypeScriptLocalFlowsToInMemoryReport {
    let mut report = Mvp4TypeScriptLocalFlowsToInMemoryReport {
        status: scope_report.status.clone(),
        eligible: scope_report.eligible,
        exclusion_reason: scope_report.exclusion_reason.clone(),
        source_role: scope_report.source_role,
        parser_status: scope_report.parser_status.clone(),
        candidates: Vec::new(),
        assignment_flow_count: 0,
        initializer_flow_count: 0,
        unresolved_read_count: 0,
        unresolved_target_count: 0,
        omitted_edge_count: 0,
        duplicate_candidate_count: 0,
        claimable_edge_missing_span_count: 0,
        claimable_edge_missing_provenance_count: 0,
        exactness_overclaim_count: 0,
        production_micro_edge_rows: 0,
        local_flow_packet_rows: 0,
        mutation_proof_activated: false,
        flow_proof_activated: false,
    };

    if !scope_report.eligible {
        return report;
    }

    type SpanKey = (u32, Option<u32>, u32, Option<u32>);
    let declaration_index: BTreeMap<(String, SpanKey), &Mvp4TypeScriptMicroNodeCandidate> =
        inventory
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.node_kind,
                    MicroNodeKind::Parameter | MicroNodeKind::LocalBinding
                )
            })
            .map(|candidate| {
                (
                    (
                        candidate.name_or_literal.clone(),
                        mvp4_source_span_key(&candidate.source_span),
                    ),
                    candidate,
                )
            })
            .collect();
    let declarations_by_id: BTreeMap<&str, &Mvp4TypeScriptMicroNodeCandidate> = inventory
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.node_kind,
                MicroNodeKind::Parameter | MicroNodeKind::LocalBinding
            )
        })
        .map(|candidate| (candidate.micro_node_id.as_str(), candidate))
        .collect();
    let functions: BTreeMap<&str, &Mvp4TypeScriptFunctionScope> = scope_report
        .functions
        .iter()
        .map(|function| (function.function_identity.as_str(), function))
        .collect();

    report.unresolved_read_count = reads.unresolved_read_count;
    report.omitted_edge_count += reads.omitted_edge_count;
    report.omitted_edge_count += writes.omitted_edge_count;

    // Index reads by enclosing function once (preserving candidate order) so the
    // per-target read scan below is O(reads_in_function), not O(reads_in_file).
    // Without this the target x file-reads loop is quadratic in file size
    // (measured P2 baseline: ~39 s to extract a 175 KB file); grouping keeps the
    // exact same (target, read) visitation order, so occurrence_index — and thus
    // every derived micro_edge_id — is byte-identical to the unindexed scan.
    let mut reads_by_function: BTreeMap<&str, Vec<&MicroEdgeCandidate>> = BTreeMap::new();
    for read_edge in &reads.candidates {
        reads_by_function
            .entry(read_edge.function_identity.as_str())
            .or_default()
            .push(read_edge);
    }

    let write_edges_by_target_and_lhs: BTreeMap<(&str, SpanKey), &MicroEdgeCandidate> = writes
        .candidates
        .iter()
        .map(|edge| {
            (
                (
                    edge.tail_micro_node_id.as_str(),
                    mvp4_source_span_key(&edge.relation_source_span),
                ),
                edge,
            )
        })
        .collect();

    let root = parsed.tree().root_node();
    let function_nodes_by_span =
        index_mvp4_ts_function_nodes_by_span(root, &parsed.repo_relative_path);
    let mut targets = Vec::new();
    for function in &scope_report.functions {
        let Some(function_node) = function_nodes_by_span
            .get(&mvp4_source_span_key(&function.source_span))
            .copied()
        else {
            continue;
        };
        let Some(body_node) = function_node.child_by_field_name("body") else {
            continue;
        };
        collect_mvp4_ts_local_flow_targets(body_node, parsed, source, function, &mut targets);
    }

    let mut seen_edge_ids = BTreeSet::new();
    let mut occurrence_index = 0u32;
    for target in &targets {
        let Some(function) = functions.get(target.enclosing_function_identity.as_str()) else {
            continue;
        };
        let resolution = resolve_mvp4_ts_local_binding(function, &target.name, &target.lhs_span);
        let Mvp4TypeScriptBindingResolution {
            status: Mvp4TypeScriptBindingResolutionStatus::Resolved,
            binding_source_span: Some(binding_span),
            ..
        } = resolution
        else {
            report.unresolved_target_count += 1;
            continue;
        };

        let target_key = (target.name.clone(), mvp4_source_span_key(&binding_span));
        let Some(target_declaration) = declaration_index.get(&target_key) else {
            report.omitted_edge_count += 1;
            continue;
        };
        if target.parse_recovery || target_declaration.parse_recovery {
            continue;
        }

        let assignment_write_edge = if target.kind == Mvp4TsFlowTargetKind::Assignment {
            let key = (
                target_declaration.micro_node_id.as_str(),
                mvp4_source_span_key(&target.lhs_span),
            );
            match write_edges_by_target_and_lhs.get(&key) {
                Some(edge) => Some(*edge),
                None => {
                    report.omitted_edge_count += 1;
                    continue;
                }
            }
        } else {
            None
        };

        let function_reads: &[&MicroEdgeCandidate] = reads_by_function
            .get(target.enclosing_function_identity.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        for read_edge in function_reads
            .iter()
            .copied()
            .filter(|edge| mvp4_source_span_contains(&target.value_span, &edge.head_source_span))
        {
            let Some(source_declaration) =
                declarations_by_id.get(read_edge.tail_micro_node_id.as_str())
            else {
                report.omitted_edge_count += 1;
                continue;
            };
            if source_declaration.parse_recovery {
                continue;
            }
            if !mvp4_source_span_is_bounded(&target.value_span)
                || !mvp4_source_span_is_bounded(&source_declaration.source_span)
                || !mvp4_source_span_is_bounded(&target_declaration.source_span)
            {
                report.claimable_edge_missing_span_count += 1;
                continue;
            }

            let mut source_fact_ids = vec![read_edge.micro_edge_id.clone()];
            let mut source_spans = vec![
                read_edge.relation_source_span.clone(),
                target.relation_span.clone(),
                target.value_span.clone(),
                target_declaration.source_span.clone(),
            ];
            let write_fact_label = if let Some(write_edge) = assignment_write_edge {
                source_fact_ids.push(write_edge.micro_edge_id.clone());
                source_spans.push(write_edge.relation_source_span.clone());
                "local_writes_edge"
            } else {
                source_fact_ids.push(target_declaration.micro_node_id.clone());
                "local_binding_initializer"
            };

            let provenance = MicroFactProvenance {
                derivation_kind: MicroDerivationKind::LocalAssignmentChainDerivation,
                source_fact_ids,
                source_spans,
                extractor_or_adapter_version: MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION.to_string(),
                exactness: MicroExactness::DerivedWithProvenance,
                limitations: vec![
                    "function_local_only".to_string(),
                    "syntactic_rhs_read_to_write_target".to_string(),
                    "derived_from_resolver_proven_local_reads".to_string(),
                    format!("target_fact_source:{write_fact_label}"),
                    "does_not_prove_runtime_value".to_string(),
                    "does_not_prove_branch_feasibility".to_string(),
                    "does_not_generate_local_flow_packet".to_string(),
                    "does_not_activate_mutation_proof_or_flow_proof".to_string(),
                ],
            };
            if validate_micro_fact_provenance(&provenance).is_err() {
                report.claimable_edge_missing_provenance_count += 1;
                continue;
            }

            let identity = MicroEdgeIdentityInput {
                repo_relative_path: target_declaration.repo_relative_path.clone(),
                language: format!(
                    "{}@{}",
                    target_declaration.language, target_declaration.frontend
                ),
                kind: MicroEdgeKind::LocalFlowsTo,
                function_entity_id: Some(target.enclosing_function_identity.clone()),
                scope_path: target_declaration.scope_path.clone(),
                source_micro_node_id: source_declaration.micro_node_id.clone(),
                target_micro_node_id: target_declaration.micro_node_id.clone(),
                structural_path: vec![
                    "relation:local_flows_to".to_string(),
                    format!("frontend:{}", target_declaration.frontend),
                    format!("target_kind:{}", target.kind.as_str()),
                    format!("source_read_edge:{}", read_edge.micro_edge_id),
                    format!("target_binding:{}", target_declaration.micro_node_id),
                    format!(
                        "target_span:{}",
                        compact_identity_hash(&format!("{:?}", target.relation_span))
                    ),
                ],
                occurrence_index,
                source_span: Some(target.value_span.clone()),
            };
            occurrence_index = occurrence_index.saturating_add(1);
            let micro_edge_id = stable_micro_edge_id(&identity);
            if !seen_edge_ids.insert(micro_edge_id.clone()) {
                report.duplicate_candidate_count += 1;
                continue;
            }

            report.candidates.push(MicroEdgeCandidate {
                micro_edge_id,
                micro_edge_kind: MicroEdgeKind::LocalFlowsTo,
                head_micro_node_id: source_declaration.micro_node_id.clone(),
                tail_micro_node_id: target_declaration.micro_node_id.clone(),
                repo_relative_path: target_declaration.repo_relative_path.clone(),
                function_identity: target.enclosing_function_identity.clone(),
                relation_source_span: target.value_span.clone(),
                head_source_span: source_declaration.source_span.clone(),
                tail_source_span: target_declaration.source_span.clone(),
                provenance,
                exactness: MicroExactness::DerivedWithProvenance,
                claimability: MVP4_2B_LOCAL_FLOWS_TO_CLAIMABILITY.to_string(),
                language: target_declaration.language.clone(),
                frontend: target_declaration.frontend.clone(),
                source_role: target_declaration.source_role,
                row_schema_version: MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION,
                payload_version: MVP4_2_MICRO_EDGE_PAYLOAD_VERSION,
                extraction_version: MVP4_2B_LOCAL_FLOWS_TO_EXTRACTION_VERSION.to_string(),
                cap_omission_state: None,
            });

            match target.kind {
                Mvp4TsFlowTargetKind::Assignment => report.assignment_flow_count += 1,
                Mvp4TsFlowTargetKind::Initializer => report.initializer_flow_count += 1,
            }
        }
    }

    sort_mvp4_micro_edge_candidates(&mut report.candidates);
    report.exactness_overclaim_count = report
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.exactness != MicroExactness::DerivedWithProvenance
                || candidate.provenance.exactness != MicroExactness::DerivedWithProvenance
                || candidate.provenance.derivation_kind
                    != MicroDerivationKind::LocalAssignmentChainDerivation
        })
        .count() as u64;
    report
}

pub fn emit_mvp4_typescript_property_access_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptPropertyAccessCandidateReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    emit_mvp4_typescript_property_access_candidates_with_scopes(parsed, source, &scope_report)
}

fn emit_mvp4_typescript_property_access_candidates_with_scopes(
    parsed: &ParsedFile,
    source: &str,
    scope_report: &Mvp4TypeScriptScopeDiscoveryReport,
) -> Mvp4TypeScriptPropertyAccessCandidateReport {
    let mut report = Mvp4TypeScriptPropertyAccessCandidateReport {
        status: scope_report.status.clone(),
        eligible: scope_report.eligible,
        exclusion_reason: scope_report.exclusion_reason.clone(),
        source_role: scope_report.source_role,
        parser_status: scope_report.parser_status.clone(),
        candidates: Vec::new(),
        duplicate_candidate_count: 0,
        source_span_failures: 0,
        binding_overclaim_count: 0,
        semantic_overclaim_count: 0,
        production_persistence_enabled: false,
        production_micro_node_rows: 0,
        micro_edge_rows: 0,
        flow_proof_activated: false,
    };

    if !scope_report.eligible {
        return report;
    }

    let root = parsed.tree().root_node();
    let function_nodes_by_span =
        index_mvp4_ts_function_nodes_by_span(root, &parsed.repo_relative_path);
    let mut occurrence_counts = BTreeMap::<(MicroNodeKind, Vec<String>, String), u32>::new();
    for function in &scope_report.functions {
        let Some(function_node) = function_nodes_by_span
            .get(&mvp4_source_span_key(&function.source_span))
            .copied()
        else {
            continue;
        };
        let Some(body_node) = function_node.child_by_field_name("body") else {
            continue;
        };
        collect_mvp4_ts_property_access_candidates(
            body_node,
            parsed,
            source,
            function,
            &mut occurrence_counts,
            &mut report.candidates,
        );
    }

    report.candidates.sort_by(|left, right| {
        left.source_span
            .start_line
            .cmp(&right.source_span.start_line)
            .then_with(|| {
                left.source_span
                    .start_column
                    .cmp(&right.source_span.start_column)
            })
            .then_with(|| left.node_kind.cmp(&right.node_kind))
            .then_with(|| left.name_or_literal.cmp(&right.name_or_literal))
            .then_with(|| left.micro_node_id.cmp(&right.micro_node_id))
    });

    let mut ids = BTreeSet::new();
    for candidate in &report.candidates {
        if !ids.insert(candidate.micro_node_id.clone()) {
            report.duplicate_candidate_count += 1;
        }
        if !mvp4_source_span_is_bounded(&candidate.source_span) {
            report.source_span_failures += 1;
        }
        if candidate.symbol_binding_id.is_some() {
            report.binding_overclaim_count += 1;
        }
        if !candidate
            .provenance
            .limitations
            .contains(&"does_not_claim_route_auth_sanitizer_or_security_semantics".to_string())
            || !candidate
                .provenance
                .limitations
                .contains(&"does_not_claim_runtime_property".to_string())
        {
            report.semantic_overclaim_count += 1;
        }
    }
    report
}

pub fn mvp4_typescript_first_slice_cap_policy() -> Mvp4TypeScriptMicroNodeCapPolicy {
    Mvp4TypeScriptMicroNodeCapPolicy {
        max_micro_nodes_per_function: MVP4_TS_MAX_MICRO_NODES_PER_FUNCTION,
        max_micro_nodes_per_file: MVP4_TS_MAX_MICRO_NODES_PER_FILE,
        per_kind_nodes_per_function: BTreeMap::from([
            (
                MicroNodeKind::FunctionFrame,
                MVP4_TS_MAX_FUNCTION_FRAME_PER_FUNCTION,
            ),
            (MicroNodeKind::Parameter, MVP4_TS_MAX_PARAMETER_PER_FUNCTION),
            (
                MicroNodeKind::LocalBinding,
                MVP4_TS_MAX_LOCAL_BINDING_PER_FUNCTION,
            ),
            (
                MicroNodeKind::AssignmentSite,
                MVP4_TS_MAX_ASSIGNMENT_SITE_PER_FUNCTION,
            ),
            (
                MicroNodeKind::ReturnSite,
                MVP4_TS_MAX_RETURN_SITE_PER_FUNCTION,
            ),
            (MicroNodeKind::CallSite, MVP4_TS_MAX_CALL_SITE_PER_FUNCTION),
            (
                MicroNodeKind::PropertyAccess,
                MVP4_TS_MAX_PROPERTY_ACCESS_PER_FUNCTION,
            ),
        ]),
        max_name_or_literal_bytes: MVP4_TS_MAX_NAME_OR_LITERAL_BYTES,
        max_parser_recovery_derived_nodes: MVP4_TS_MAX_PARSER_RECOVERY_DERIVED_NODES,
        max_omission_details_in_compact_status: MVP4_TS_MAX_OMISSION_DETAILS_IN_COMPACT_STATUS,
    }
}

pub fn emit_mvp4_typescript_in_memory_first_slice_inventory(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptInMemoryInventoryReport {
    emit_mvp4_typescript_in_memory_first_slice_inventory_with_policy(
        parsed,
        source,
        &mvp4_typescript_first_slice_cap_policy(),
    )
}

fn emit_mvp4_typescript_in_memory_first_slice_inventory_with_policy(
    parsed: &ParsedFile,
    source: &str,
    policy: &Mvp4TypeScriptMicroNodeCapPolicy,
) -> Mvp4TypeScriptInMemoryInventoryReport {
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    emit_mvp4_typescript_in_memory_first_slice_inventory_with_scopes_and_policy(
        parsed,
        source,
        policy,
        &scope_report,
    )
}

fn emit_mvp4_typescript_in_memory_first_slice_inventory_with_scopes_and_policy(
    parsed: &ParsedFile,
    source: &str,
    policy: &Mvp4TypeScriptMicroNodeCapPolicy,
    scope_report: &Mvp4TypeScriptScopeDiscoveryReport,
) -> Mvp4TypeScriptInMemoryInventoryReport {
    // Share one scope discovery across the three inventory inputs; each used to
    // recompute it (and the full entity extraction inside it) on its own.
    let core =
        emit_mvp4_typescript_core_declaration_candidates_with_scopes(parsed, source, scope_report);
    let operations =
        emit_mvp4_typescript_operation_site_candidates_with_scopes(parsed, source, scope_report);
    let properties =
        emit_mvp4_typescript_property_access_candidates_with_scopes(parsed, source, scope_report);
    let mut report = Mvp4TypeScriptInMemoryInventoryReport {
        status: core.status.clone(),
        eligible: core.eligible,
        exclusion_reason: core.exclusion_reason.clone(),
        source_role: core.source_role,
        parser_status: core.parser_status.clone(),
        candidates: Vec::new(),
        omitted_count: 0,
        cap_hits: Vec::new(),
        completeness_label: "complete".to_string(),
        duplicate_candidate_count: 0,
        stable_id_collision_count: 0,
        claimable_node_missing_span_count: 0,
        source_span_failures: 0,
        binding_overclaim_count: 0,
        semantic_overclaim_count: 0,
        production_persistence_enabled: false,
        production_micro_node_rows: 0,
        micro_edge_rows: 0,
        flow_proof_activated: false,
    };

    if !report.eligible {
        return report;
    }

    let mut candidates = Vec::new();
    candidates.extend(core.candidates);
    candidates.extend(operations.candidates);
    candidates.extend(properties.candidates);
    for candidate in &mut candidates {
        candidate.name_or_literal = mvp4_ts_bound_name_or_literal(
            &candidate.name_or_literal,
            policy.max_name_or_literal_bytes,
        );
    }
    sort_mvp4_ts_candidates(&mut candidates);

    apply_mvp4_ts_micro_node_caps(&mut report, candidates, policy);
    finalize_mvp4_ts_inventory_counts(&mut report);
    report
}

pub fn emit_mvp4_typescript_local_returns_to_in_memory_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptLocalReturnsToInMemoryReport {
    emit_mvp4_typescript_local_returns_to_in_memory_candidates_with_policy(
        parsed,
        source,
        &mvp4_typescript_first_slice_cap_policy(),
    )
}

/// MVP4.2 + MVP4.2b combined micro-edge candidates for a file: LOCAL_RETURNS_TO
/// containment, exact resolver-backed LOCAL_READS / LOCAL_WRITES, and derived
/// LOCAL_FLOWS_TO. Returns the LOCAL_RETURNS_TO report shape with the additional
/// edges merged into `candidates` and omitted counts summed, so the existing
/// persistence call sites consume all active relations through one value.
pub fn emit_mvp4_2_typescript_micro_edge_candidates(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptLocalReturnsToInMemoryReport {
    emit_mvp4_typescript_micro_flow_extraction_context(parsed, source).micro_edges
}

pub fn emit_mvp4_typescript_micro_flow_extraction_context(
    parsed: &ParsedFile,
    source: &str,
) -> Mvp4TypeScriptMicroFlowExtractionContext {
    let policy = mvp4_typescript_first_slice_cap_policy();
    let scope_report = discover_mvp4_typescript_scopes(parsed, source);
    let inventory = emit_mvp4_typescript_in_memory_first_slice_inventory_with_scopes_and_policy(
        parsed,
        source,
        &policy,
        &scope_report,
    );
    let value_uses =
        emit_mvp4_typescript_value_use_candidates_with_scopes(parsed, source, &scope_report);
    let persistable_value_uses = mvp4_2b_persistable_value_use_candidates_from_report(&value_uses);
    let persistable_value_use_ids: BTreeSet<String> = persistable_value_uses
        .iter()
        .map(|candidate| candidate.micro_node_id.clone())
        .collect();
    let writes = emit_mvp4_typescript_local_writes_in_memory_candidates_with_context(
        parsed,
        source,
        &scope_report,
        &inventory,
    );
    let reads = emit_mvp4_typescript_local_reads_in_memory_candidates_with_context(
        &scope_report,
        &inventory,
        &value_uses,
    );
    let flows = emit_mvp4_typescript_local_flows_to_in_memory_candidates_with_context(
        parsed,
        source,
        &scope_report,
        &inventory,
        &reads,
        &writes,
    );
    let mut micro_edges = emit_mvp4_typescript_local_returns_to_in_memory_candidates_with_inventory(
        parsed, &inventory,
    );

    // MVP4.2b: merge exact LOCAL_WRITES edges. Both endpoints (AssignmentSite head,
    // Parameter/LocalBinding tail) are looked up in — and the edge is skipped when
    // either is absent from — the same capped MVP4.1 inventory that is persisted,
    // so ast_micro_edge endpoint integrity always holds for writes.
    micro_edges.omitted_edge_count += writes.omitted_edge_count;
    micro_edges.duplicate_candidate_count += writes.duplicate_candidate_count;
    micro_edges.claimable_edge_missing_span_count += writes.claimable_edge_missing_span_count;
    micro_edges.claimable_edge_missing_provenance_count +=
        writes.claimable_edge_missing_provenance_count;
    micro_edges.flow_semantic_overclaim_count += writes.flow_semantic_overclaim_count;
    micro_edges.candidates.extend(writes.candidates);

    // MVP4.2b: merge exact LOCAL_READS edges. The head is a ValueUse node, but only
    // ValueUse nodes that survive the persistable per-function cap are retained as
    // ast_micro_nodes (the index persists the SAME capped set this filter computes).
    // Filter read edges to those whose head is persistable so persisted
    // ast_micro_edge endpoint integrity holds; a read whose head was cap-dropped is
    // counted as omitted (a known cap limitation, not an integrity error). The tail
    // (Parameter/LocalBinding) is already constrained to the persisted inventory by
    // the reads emitter.
    micro_edges.omitted_edge_count += reads.omitted_edge_count;
    micro_edges.duplicate_candidate_count += reads.duplicate_candidate_count;
    micro_edges.claimable_edge_missing_span_count += reads.claimable_edge_missing_span_count;
    micro_edges.claimable_edge_missing_provenance_count +=
        reads.claimable_edge_missing_provenance_count;
    micro_edges.flow_semantic_overclaim_count += reads.flow_semantic_overclaim_count;
    for candidate in reads.candidates {
        if persistable_value_use_ids.contains(&candidate.head_micro_node_id) {
            micro_edges.candidates.push(candidate);
        } else {
            micro_edges.omitted_edge_count += 1;
        }
    }

    // MVP4.2b B4: merge derived LOCAL_FLOWS_TO edges. Endpoints are retained
    // Parameter/LocalBinding nodes, and every edge carries derived provenance
    // from exact local read/write facts. This still does not create packet rows
    // or activate flow_proof; it only persists the derived relation candidate.
    micro_edges.omitted_edge_count += flows.omitted_edge_count;
    micro_edges.duplicate_candidate_count += flows.duplicate_candidate_count;
    micro_edges.claimable_edge_missing_span_count += flows.claimable_edge_missing_span_count;
    micro_edges.claimable_edge_missing_provenance_count +=
        flows.claimable_edge_missing_provenance_count;
    micro_edges.candidates.extend(flows.candidates);

    sort_mvp4_micro_edge_candidates(&mut micro_edges.candidates);
    Mvp4TypeScriptMicroFlowExtractionContext {
        inventory,
        persistable_value_uses,
        micro_edges,
    }
}

fn emit_mvp4_typescript_local_returns_to_in_memory_candidates_with_policy(
    parsed: &ParsedFile,
    source: &str,
    policy: &Mvp4TypeScriptMicroNodeCapPolicy,
) -> Mvp4TypeScriptLocalReturnsToInMemoryReport {
    let inventory =
        emit_mvp4_typescript_in_memory_first_slice_inventory_with_policy(parsed, source, policy);
    emit_mvp4_typescript_local_returns_to_in_memory_candidates_with_inventory(parsed, &inventory)
}

fn emit_mvp4_typescript_local_returns_to_in_memory_candidates_with_inventory(
    parsed: &ParsedFile,
    inventory: &Mvp4TypeScriptInMemoryInventoryReport,
) -> Mvp4TypeScriptLocalReturnsToInMemoryReport {
    let mut report = Mvp4TypeScriptLocalReturnsToInMemoryReport {
        status: inventory.status.clone(),
        eligible: inventory.eligible,
        exclusion_reason: inventory.exclusion_reason.clone(),
        source_role: inventory.source_role,
        parser_status: inventory.parser_status.clone(),
        candidates: Vec::new(),
        diagnostics: Vec::new(),
        omitted_edge_count: mvp4_local_returns_to_omitted_return_site_count(&inventory),
        completeness_label: inventory.completeness_label.clone(),
        duplicate_candidate_count: 0,
        cross_function_edge_count: 0,
        cross_file_edge_count: 0,
        claimable_edge_missing_span_count: 0,
        claimable_edge_missing_provenance_count: 0,
        flow_semantic_overclaim_count: 0,
        production_micro_edge_rows: 0,
        local_flow_packet_rows: 0,
        mutation_proof_activated: false,
        flow_proof_activated: false,
    };

    if !inventory.eligible {
        report.completeness_label = "not_applicable".to_string();
        return report;
    }

    let function_frames = inventory
        .candidates
        .iter()
        .filter(|candidate| candidate.node_kind == MicroNodeKind::FunctionFrame)
        .map(|candidate| (candidate.enclosing_function_identity.clone(), candidate))
        .collect::<BTreeMap<_, _>>();
    let retained_return_site_count = inventory
        .candidates
        .iter()
        .filter(|candidate| candidate.node_kind == MicroNodeKind::ReturnSite)
        .count();

    if inventory.parser_status == "parsed_with_syntax_diagnostics"
        && retained_return_site_count == 0
    {
        report.diagnostics.push(mvp4_local_returns_to_diagnostic(
            "parser_recovery_no_retained_return_site",
            "unsupported_degraded_condition",
            "Parser recovery prevented retention of a claimable ReturnSite endpoint, so exact LOCAL_RETURNS_TO is unavailable.",
            None,
            None,
            vec![
                "current_return_site".to_string(),
                "no_parser_recovery_ambiguity".to_string(),
            ],
            0,
            "unsupported_or_unknown",
        ));
    }

    for return_site in inventory
        .candidates
        .iter()
        .filter(|candidate| candidate.node_kind == MicroNodeKind::ReturnSite)
    {
        if return_site.parse_recovery {
            report.diagnostics.push(mvp4_local_returns_to_diagnostic(
                "parser_recovery_return_site",
                "unsupported_degraded_condition",
                "ReturnSite was retained only as parser-recovery evidence, so exact LOCAL_RETURNS_TO is omitted.",
                Some(return_site),
                None,
                vec!["no_parser_recovery_ambiguity".to_string()],
                0,
                "unsupported_or_unknown",
            ));
            continue;
        }

        let Some(function_frame) = function_frames.get(&return_site.enclosing_function_identity)
        else {
            let omitted_by_cap = mvp4_local_returns_to_cap_may_have_omitted_function_frame(
                &inventory,
                &return_site.enclosing_function_identity,
            );
            if omitted_by_cap {
                report.omitted_edge_count += 1;
            }
            report.diagnostics.push(mvp4_local_returns_to_diagnostic(
                if omitted_by_cap {
                    "function_frame_endpoint_omitted_by_cap"
                } else {
                    "missing_function_frame_endpoint"
                },
                if omitted_by_cap {
                    "unsupported_degraded_condition"
                } else {
                    "proof_integrity_diagnostic"
                },
                if omitted_by_cap {
                    "Retained ReturnSite has no retained FunctionFrame endpoint because the node inventory was cap-truncated."
                } else {
                    "Retained ReturnSite has no matching retained FunctionFrame endpoint in the same function identity domain."
                },
                Some(return_site),
                None,
                vec!["current_function_frame".to_string()],
                u64::from(omitted_by_cap),
                if omitted_by_cap {
                    "unsupported_or_unknown"
                } else {
                    "reindex_or_repair"
                },
            ));
            continue;
        };

        let provenance = mvp4_local_returns_to_provenance(return_site, function_frame);
        let direct_ast_provenance = validate_micro_fact_provenance(&provenance).is_ok();
        let exactness_input = LocalReturnsToExactnessInput {
            supported_typescript_production_ts_file: parsed.language == SourceLanguage::TypeScript
                && is_mvp4_exact_typescript_ts_path(&parsed.repo_relative_path)
                && inventory.source_role == MicroSourceRole::Production,
            current_return_site: return_site.node_kind == MicroNodeKind::ReturnSite
                && return_site.extraction_version == MVP4_TS_MICRO_NODE_EXTRACTION_VERSION,
            current_function_frame: function_frame.node_kind == MicroNodeKind::FunctionFrame
                && function_frame.extraction_version == MVP4_TS_MICRO_NODE_EXTRACTION_VERSION,
            return_site_source_span: mvp4_source_span_is_bounded(&return_site.source_span),
            function_frame_source_span: mvp4_source_span_is_bounded(&function_frame.source_span),
            deterministic_nearest_enclosing_function_ownership: return_site
                .enclosing_function_identity
                == function_frame.enclosing_function_identity,
            identical_normalized_file_identity: return_site.repo_relative_path
                == function_frame.repo_relative_path,
            identical_enclosing_function_identity_domain: return_site.enclosing_function_identity
                == function_frame.enclosing_function_identity,
            endpoints_retained_after_caps: true,
            current_extraction_versions: return_site.row_schema_version
                == MVP4_TS_MICRO_NODE_ROW_SCHEMA_VERSION
                && function_frame.row_schema_version == MVP4_TS_MICRO_NODE_ROW_SCHEMA_VERSION
                && return_site.payload_version == MVP4_TS_MICRO_NODE_PAYLOAD_VERSION
                && function_frame.payload_version == MVP4_TS_MICRO_NODE_PAYLOAD_VERSION,
            claimable_lifecycle_passport: return_site.claimability.starts_with("claimable_")
                && function_frame.claimability.starts_with("claimable_"),
            direct_ast_provenance,
            no_parser_recovery_ambiguity: !return_site.parse_recovery
                && !function_frame.parse_recovery,
        };
        let decision = decide_local_returns_to_exactness(&exactness_input);
        if !decision.claimable {
            report.diagnostics.push(mvp4_local_returns_to_diagnostic(
                "local_returns_to_exactness_requirements_missing",
                "unsupported_degraded_condition",
                "LOCAL_RETURNS_TO exact edge requirements were not all satisfied; no proof row candidate emitted.",
                Some(return_site),
                Some(function_frame),
                decision
                    .missing_requirements
                    .iter()
                    .map(|requirement| requirement.as_str().to_string())
                    .collect(),
                0,
                "unsupported_or_unknown",
            ));
            continue;
        }

        let identity_input = LocalReturnsToIdentityContractInput {
            repo_relative_path: return_site.repo_relative_path.clone(),
            language: format!("{}@{}", return_site.language, return_site.frontend),
            function_entity_id: return_site.enclosing_function_identity.clone(),
            scope_path: function_frame.scope_path.clone(),
            head_return_site_micro_node_id: return_site.micro_node_id.clone(),
            tail_function_frame_micro_node_id: function_frame.micro_node_id.clone(),
            return_structural_path: mvp4_local_returns_to_identity_path(return_site),
            occurrence_index: return_site.occurrence_index,
            source_role: return_site.source_role,
            row_schema_version: MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION,
            payload_version: MVP4_2_MICRO_EDGE_PAYLOAD_VERSION,
            extraction_version: MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION.to_string(),
            relation_span: Some(return_site.source_span.clone()),
        };
        let edge_identity = local_returns_to_identity_input(&identity_input);
        let cap_omission_state = (inventory.omitted_count > 0).then(|| MicroEdgeCandidateCapState {
            omitted_count: inventory.omitted_count,
            reason: "retained micro-node inventory is cap-truncated; exact edge coverage is incomplete".to_string(),
            completeness_label: inventory.completeness_label.clone(),
        });

        report.candidates.push(MicroEdgeCandidate {
            micro_edge_id: stable_micro_edge_id(&edge_identity),
            micro_edge_kind: MicroEdgeKind::LocalReturnsTo,
            head_micro_node_id: return_site.micro_node_id.clone(),
            tail_micro_node_id: function_frame.micro_node_id.clone(),
            repo_relative_path: return_site.repo_relative_path.clone(),
            function_identity: return_site.enclosing_function_identity.clone(),
            relation_source_span: return_site.source_span.clone(),
            head_source_span: return_site.source_span.clone(),
            tail_source_span: function_frame.source_span.clone(),
            provenance,
            exactness: decision.exactness,
            claimability: "claimable_source_spanned_local_return_containment".to_string(),
            language: return_site.language.clone(),
            frontend: return_site.frontend.clone(),
            source_role: return_site.source_role,
            row_schema_version: MVP4_2_MICRO_EDGE_ROW_SCHEMA_VERSION,
            payload_version: MVP4_2_MICRO_EDGE_PAYLOAD_VERSION,
            extraction_version: MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION.to_string(),
            cap_omission_state,
        });
    }

    sort_mvp4_micro_edge_candidates(&mut report.candidates);
    finalize_mvp4_local_returns_to_report(&mut report);
    if report.omitted_edge_count > 0 || !report.diagnostics.is_empty() {
        report.completeness_label = "truncated_or_degraded_unknown_completeness".to_string();
    }
    report
}

fn mvp4_local_returns_to_provenance(
    return_site: &Mvp4TypeScriptMicroNodeCandidate,
    function_frame: &Mvp4TypeScriptMicroNodeCandidate,
) -> MicroFactProvenance {
    MicroFactProvenance {
        derivation_kind: MicroDerivationKind::DirectAstExtraction,
        source_fact_ids: vec![
            return_site.micro_node_id.clone(),
            function_frame.micro_node_id.clone(),
        ],
        source_spans: vec![
            return_site.source_span.clone(),
            function_frame.source_span.clone(),
        ],
        extractor_or_adapter_version: MVP4_2_LOCAL_RETURNS_TO_EXTRACTION_VERSION.to_string(),
        exactness: MicroExactness::Exact,
        limitations: vec![
            "local_ast_containment_only".to_string(),
            "does_not_prove_returned_value_flow".to_string(),
            "does_not_prove_reachability".to_string(),
            "does_not_prove_complete_return_coverage".to_string(),
            "does_not_prove_runtime_behavior".to_string(),
            "does_not_generate_local_flow_packet".to_string(),
            "does_not_activate_mutation_proof_or_flow_proof".to_string(),
        ],
    }
}

fn mvp4_local_returns_to_identity_path(
    return_site: &Mvp4TypeScriptMicroNodeCandidate,
) -> Vec<String> {
    vec![
        "relation:local_returns_to".to_string(),
        "ownership:nearest_enclosing_function".to_string(),
        format!("frontend:{}", return_site.frontend),
        format!("head_kind:{}", MicroNodeKind::ReturnSite.as_str()),
        format!("tail_kind:{}", MicroNodeKind::FunctionFrame.as_str()),
        format!("return_node:{}", return_site.micro_node_id),
    ]
}

fn mvp4_local_returns_to_diagnostic(
    diagnostic_kind: &str,
    classification: &str,
    reason: &str,
    head: Option<&Mvp4TypeScriptMicroNodeCandidate>,
    tail: Option<&Mvp4TypeScriptMicroNodeCandidate>,
    missing_requirements: Vec<String>,
    omitted_count: u64,
    recommended_action_kind: &str,
) -> Mvp4TypeScriptMicroEdgeDiagnostic {
    Mvp4TypeScriptMicroEdgeDiagnostic {
        diagnostic_kind: diagnostic_kind.to_string(),
        classification: classification.to_string(),
        reason: reason.to_string(),
        head_micro_node_id: head.map(|candidate| candidate.micro_node_id.clone()),
        tail_micro_node_id: tail.map(|candidate| candidate.micro_node_id.clone()),
        function_identity: head
            .or(tail)
            .map(|candidate| candidate.enclosing_function_identity.clone()),
        source_span: head
            .map(|candidate| candidate.source_span.clone())
            .or_else(|| tail.map(|candidate| candidate.source_span.clone())),
        missing_requirements,
        omitted_count,
        recommended_action_kind: recommended_action_kind.to_string(),
    }
}

fn mvp4_local_returns_to_omitted_return_site_count(
    inventory: &Mvp4TypeScriptInMemoryInventoryReport,
) -> u64 {
    inventory
        .cap_hits
        .iter()
        .filter(|hit| {
            hit.node_kind == Some(MicroNodeKind::ReturnSite)
                || (hit.node_kind.is_none()
                    && matches!(hit.cap_kind.as_str(), "file_total" | "function_total"))
        })
        .map(|hit| hit.omitted_count)
        .sum()
}

fn mvp4_local_returns_to_cap_may_have_omitted_function_frame(
    inventory: &Mvp4TypeScriptInMemoryInventoryReport,
    function_identity: &str,
) -> bool {
    inventory.cap_hits.iter().any(|hit| {
        hit.node_kind == Some(MicroNodeKind::FunctionFrame)
            || (hit.node_kind.is_none()
                && (hit.function_identity.as_deref() == Some(function_identity)
                    || hit.cap_kind == "file_total"))
    })
}

fn sort_mvp4_micro_edge_candidates(candidates: &mut [MicroEdgeCandidate]) {
    candidates.sort_by(|left, right| {
        left.repo_relative_path
            .cmp(&right.repo_relative_path)
            .then_with(|| left.function_identity.cmp(&right.function_identity))
            .then_with(|| {
                left.relation_source_span
                    .start_line
                    .cmp(&right.relation_source_span.start_line)
            })
            .then_with(|| {
                left.relation_source_span
                    .start_column
                    .cmp(&right.relation_source_span.start_column)
            })
            .then_with(|| left.micro_edge_kind.cmp(&right.micro_edge_kind))
            .then_with(|| left.head_micro_node_id.cmp(&right.head_micro_node_id))
            .then_with(|| left.tail_micro_node_id.cmp(&right.tail_micro_node_id))
            .then_with(|| left.micro_edge_id.cmp(&right.micro_edge_id))
    });
}

fn finalize_mvp4_local_returns_to_report(report: &mut Mvp4TypeScriptLocalReturnsToInMemoryReport) {
    let mut ids = BTreeSet::new();
    for candidate in &report.candidates {
        if !ids.insert(candidate.micro_edge_id.clone()) {
            report.duplicate_candidate_count += 1;
        }
        if candidate.repo_relative_path != candidate.head_source_span.repo_relative_path
            || candidate.repo_relative_path != candidate.tail_source_span.repo_relative_path
        {
            report.cross_file_edge_count += 1;
        }
        if candidate.head_micro_node_id == candidate.tail_micro_node_id
            || candidate.function_identity.trim().is_empty()
        {
            report.cross_function_edge_count += 1;
        }
        if candidate.claimability.starts_with("claimable_")
            && (!mvp4_source_span_is_bounded(&candidate.relation_source_span)
                || !mvp4_source_span_is_bounded(&candidate.head_source_span)
                || !mvp4_source_span_is_bounded(&candidate.tail_source_span))
        {
            report.claimable_edge_missing_span_count += 1;
        }
        if candidate.claimability.starts_with("claimable_")
            && validate_micro_fact_provenance(&candidate.provenance).is_err()
        {
            report.claimable_edge_missing_provenance_count += 1;
        }
        if mvp4_local_returns_to_overclaims_flow_semantics(candidate) {
            report.flow_semantic_overclaim_count += 1;
        }
    }
}

fn mvp4_local_returns_to_overclaims_flow_semantics(candidate: &MicroEdgeCandidate) -> bool {
    candidate.micro_edge_kind != MicroEdgeKind::LocalReturnsTo
        || candidate.claimability.contains("flow_proof")
        || candidate.claimability.contains("mutation_proof")
        || !candidate
            .provenance
            .limitations
            .contains(&"local_ast_containment_only".to_string())
        || !candidate
            .provenance
            .limitations
            .contains(&"does_not_prove_returned_value_flow".to_string())
        || !candidate
            .provenance
            .limitations
            .contains(&"does_not_prove_runtime_behavior".to_string())
}

/// One traversal collecting every TypeScript function-scope node keyed by its
/// positional source span. Replaces a per-function `find ... by span` DFS from
/// root — which made each emitter's `for function { find_by_span(root, ..) }`
/// loop O(functions * tree), i.e. quadratic in file size (measured P2 baseline:
/// ~39 s to extract a 175 KB / 200-function file) — with one O(tree) build plus
/// O(log n) lookups. The key is positional (`mvp4_source_span_key`), matching the
/// span keys carried on each `Mvp4TypeScriptFunctionScope`; spans are unique per
/// node so there are no collisions, and nested functions get their own entries.
fn index_mvp4_ts_function_nodes_by_span<'tree>(
    root: Node<'tree>,
    repo_relative_path: &str,
) -> BTreeMap<(u32, Option<u32>, u32, Option<u32>), Node<'tree>> {
    let mut map = BTreeMap::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if is_mvp4_ts_function_scope_node(node) {
            let span = source_span_for_node(repo_relative_path, node);
            map.entry(mvp4_source_span_key(&span)).or_insert(node);
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    map
}

fn collect_mvp4_ts_operation_site_candidates(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    function: &Mvp4TypeScriptFunctionScope,
    occurrence_counts: &mut BTreeMap<(MicroNodeKind, Vec<String>, String), u32>,
    candidates: &mut Vec<Mvp4TypeScriptMicroNodeCandidate>,
) {
    if node.is_error() || node.is_missing() || is_mvp4_ts_function_scope_node(node) {
        return;
    }

    let node_kind = match node.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            Some(MicroNodeKind::AssignmentSite)
        }
        "return_statement" => Some(MicroNodeKind::ReturnSite),
        "call_expression" => Some(MicroNodeKind::CallSite),
        _ => None,
    };

    if let Some(node_kind) = node_kind {
        let source_span = source_span_for_node(&parsed.repo_relative_path, node);
        let parse_recovery = function.parse_recovery || node_has_error_or_missing_descendant(node);
        let mut scope_path = function.scope_path.clone();
        scope_path.push("operation_sites".to_string());
        scope_path.push(node_kind.as_str().to_string());
        let name_or_literal = mvp4_ts_operation_site_label(node, node_kind, source);
        let structural_ast_path =
            mvp4_ts_operation_site_structural_path(function, node_kind, &name_or_literal);
        push_mvp4_ts_candidate(
            parsed,
            function,
            node_kind,
            MVP4_TS_OPERATION_SITE_EXTRACTOR,
            &scope_path,
            &structural_ast_path,
            &source_span,
            &name_or_literal,
            None,
            Some(function.function_kind.clone()),
            None,
            parse_recovery,
            occurrence_counts,
            candidates,
        );
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_mvp4_ts_function_scope_node(child) {
            continue;
        }
        collect_mvp4_ts_operation_site_candidates(
            child,
            parsed,
            source,
            function,
            occurrence_counts,
            candidates,
        );
    }
}

fn collect_mvp4_ts_property_access_candidates(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    function: &Mvp4TypeScriptFunctionScope,
    occurrence_counts: &mut BTreeMap<(MicroNodeKind, Vec<String>, String), u32>,
    candidates: &mut Vec<Mvp4TypeScriptMicroNodeCandidate>,
) {
    if node.is_error() || node.is_missing() || is_mvp4_ts_function_scope_node(node) {
        return;
    }

    if is_mvp4_ts_property_access_node(node) {
        let source_span = source_span_for_node(&parsed.repo_relative_path, node);
        let parse_recovery = function.parse_recovery || node_has_error_or_missing_descendant(node);
        let mut scope_path = function.scope_path.clone();
        scope_path.push("property_accesses".to_string());
        let name_or_literal = mvp4_ts_property_access_label(node, source);
        let structural_ast_path =
            mvp4_ts_property_access_structural_path(function, &name_or_literal);
        push_mvp4_ts_candidate(
            parsed,
            function,
            MicroNodeKind::PropertyAccess,
            MVP4_TS_PROPERTY_ACCESS_EXTRACTOR,
            &scope_path,
            &structural_ast_path,
            &source_span,
            &name_or_literal,
            None,
            Some(function.function_kind.clone()),
            Some(mvp4_ts_property_access_shape(node, &name_or_literal).to_string()),
            parse_recovery,
            occurrence_counts,
            candidates,
        );
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_mvp4_ts_function_scope_node(child) {
            continue;
        }
        collect_mvp4_ts_property_access_candidates(
            child,
            parsed,
            source,
            function,
            occurrence_counts,
            candidates,
        );
    }
}

fn is_mvp4_ts_property_access_node(node: Node<'_>) -> bool {
    matches!(node.kind(), "member_expression" | "subscript_expression")
}

fn mvp4_ts_property_access_structural_path(
    function: &Mvp4TypeScriptFunctionScope,
    name_or_literal: &str,
) -> Vec<String> {
    function
        .structural_ast_path
        .iter()
        .cloned()
        .chain([
            "property_access".to_string(),
            format!("syntax:{}", compact_identity_hash(name_or_literal)),
        ])
        .collect()
}

fn mvp4_ts_property_access_label(node: Node<'_>, source: &str) -> String {
    node_text(node, source)
        .map(|text| compact_extracted_label(&text, node))
        .unwrap_or_else(|| format!("property_access@{}", node.start_byte()))
}

fn mvp4_ts_property_access_shape(node: Node<'_>, name_or_literal: &str) -> &'static str {
    if node.kind() == "subscript_expression" || name_or_literal.contains('[') {
        "computed_member"
    } else if name_or_literal.contains("?.") {
        "optional_static_member"
    } else {
        "static_member"
    }
}

fn mvp4_ts_operation_site_structural_path(
    function: &Mvp4TypeScriptFunctionScope,
    node_kind: MicroNodeKind,
    name_or_literal: &str,
) -> Vec<String> {
    function
        .structural_ast_path
        .iter()
        .cloned()
        .chain([
            "operation_site".to_string(),
            node_kind.as_str().to_string(),
            format!("syntax:{}", compact_identity_hash(name_or_literal)),
        ])
        .collect()
}

fn mvp4_ts_operation_site_label(node: Node<'_>, node_kind: MicroNodeKind, source: &str) -> String {
    match node_kind {
        MicroNodeKind::AssignmentSite => node_text(node, source)
            .map(|text| format!("assignment:{}", compact_extracted_label(&text, node)))
            .unwrap_or_else(|| format!("assignment@{}", node.start_byte())),
        MicroNodeKind::ReturnSite => node_text(node, source)
            .map(|text| format!("return:{}", compact_extracted_label(&text, node)))
            .unwrap_or_else(|| format!("return@{}", node.start_byte())),
        MicroNodeKind::CallSite => {
            let label = node
                .child_by_field_name("function")
                .and_then(|callee| node_text(callee, source))
                .or_else(|| {
                    node_text(node, source).map(|text| {
                        text.split_once('(')
                            .map(|(callee, _)| callee.trim().to_string())
                            .unwrap_or(text)
                    })
                })
                .map(|text| compact_extracted_label(&text, node))
                .unwrap_or_else(|| format!("call@{}", node.start_byte()));
            format!("call:{label}")
        }
        _ => compact_extracted_label(node.kind(), node),
    }
}

fn push_mvp4_ts_candidate(
    parsed: &ParsedFile,
    function: &Mvp4TypeScriptFunctionScope,
    node_kind: MicroNodeKind,
    extractor_version: &str,
    scope_path: &[String],
    structural_ast_path: &[String],
    source_span: &SourceSpan,
    name_or_literal: &str,
    symbol_binding_id: Option<String>,
    function_kind: Option<String>,
    declaration_kind: Option<String>,
    parse_recovery: bool,
    occurrence_counts: &mut BTreeMap<(MicroNodeKind, Vec<String>, String), u32>,
    candidates: &mut Vec<Mvp4TypeScriptMicroNodeCandidate>,
) {
    let occurrence_key = (
        node_kind,
        scope_path.to_vec(),
        name_or_literal.trim().to_string(),
    );
    let occurrence_index = occurrence_counts.entry(occurrence_key).or_insert(0);
    let current_occurrence = *occurrence_index;
    *occurrence_index += 1;

    let graph_function_entity_id = function
        .existing_entity_link
        .as_ref()
        .map(|link| link.entity_id.clone());
    let identity_binding_id = if node_kind == MicroNodeKind::FunctionFrame {
        None
    } else {
        symbol_binding_id.clone()
    };
    let exactness = if parse_recovery {
        MicroExactness::Unknown
    } else {
        MicroExactness::Exact
    };
    let identity = MicroNodeIdentityInput {
        repo_relative_path: normalize_repo_relative_path(&parsed.repo_relative_path),
        language: MVP4_TS_MICRO_NODE_LANGUAGE.to_string(),
        kind: node_kind,
        function_entity_id: Some(function.function_identity.clone()),
        scope_path: scope_path.to_vec(),
        binding_id: identity_binding_id,
        symbol: Some(name_or_literal.trim().to_string()),
        structural_path: structural_ast_path.to_vec(),
        occurrence_index: current_occurrence,
        source_span: Some(source_span.clone()),
        parse_recovery,
    };
    let micro_node_id = stable_micro_node_id(&identity);
    let provenance = MicroFactProvenance {
        derivation_kind: MicroDerivationKind::DirectAstExtraction,
        source_fact_ids: Vec::new(),
        source_spans: vec![source_span.clone()],
        extractor_or_adapter_version: extractor_version.to_string(),
        exactness,
        limitations: mvp4_ts_candidate_limitations(node_kind),
    };

    candidates.push(Mvp4TypeScriptMicroNodeCandidate {
        micro_node_id,
        node_kind,
        repo_relative_path: normalize_repo_relative_path(&parsed.repo_relative_path),
        language: MVP4_TS_MICRO_NODE_LANGUAGE.to_string(),
        frontend: MVP4_TS_MICRO_NODE_FRONTEND.to_string(),
        enclosing_function_identity: function.function_identity.clone(),
        function_entity_id: graph_function_entity_id,
        function_kind,
        scope_path: scope_path.to_vec(),
        structural_ast_path: structural_ast_path.to_vec(),
        occurrence_index: current_occurrence,
        source_span: source_span.clone(),
        name_or_literal: name_or_literal.trim().to_string(),
        symbol_binding_id,
        source_role: function.source_role,
        exactness,
        claimability: mvp4_ts_candidate_claimability(node_kind, name_or_literal, parse_recovery),
        parse_recovery,
        declaration_kind,
        row_schema_version: MVP4_TS_MICRO_NODE_ROW_SCHEMA_VERSION,
        payload_version: MVP4_TS_MICRO_NODE_PAYLOAD_VERSION,
        extraction_version: MVP4_TS_MICRO_NODE_EXTRACTION_VERSION.to_string(),
        provenance,
    });
}

fn mvp4_ts_candidate_claimability(
    node_kind: MicroNodeKind,
    name_or_literal: &str,
    parse_recovery: bool,
) -> String {
    if parse_recovery {
        return "non_claimable_recovery_candidate".to_string();
    }
    match node_kind {
        MicroNodeKind::FunctionFrame => "claimable_source_spanned_function_body_existence",
        MicroNodeKind::Parameter => "claimable_source_spanned_parameter_existence",
        MicroNodeKind::LocalBinding => "claimable_source_spanned_local_binding_existence",
        MicroNodeKind::AssignmentSite => "claimable_source_spanned_assignment_expression_existence",
        MicroNodeKind::ReturnSite => "claimable_source_spanned_return_statement_existence",
        MicroNodeKind::CallSite => "claimable_source_spanned_call_expression_existence",
        MicroNodeKind::PropertyAccess if name_or_literal.contains('[') => {
            "claimable_source_spanned_property_access_existence"
        }
        MicroNodeKind::PropertyAccess => {
            "claimable_source_spanned_property_access_and_static_property_token_existence"
        }
        MicroNodeKind::ValueUse => "claimable_source_spanned_value_use_existence",
        _ => "non_claimable_out_of_slice",
    }
    .to_string()
}

fn mvp4_ts_candidate_limitations(node_kind: MicroNodeKind) -> Vec<String> {
    match node_kind {
        MicroNodeKind::FunctionFrame => vec![
            "does_not_claim_call_behavior".to_string(),
            "does_not_claim_dataflow".to_string(),
            "does_not_claim_runtime_invocation".to_string(),
            "does_not_claim_route_auth_or_security_semantics".to_string(),
        ],
        MicroNodeKind::Parameter => vec![
            "does_not_claim_resolver_binding_identity_without_symbol_binding_id".to_string(),
            "does_not_claim_reads_writes_or_flow".to_string(),
        ],
        MicroNodeKind::LocalBinding => vec![
            "does_not_claim_resolver_binding_identity_without_symbol_binding_id".to_string(),
            "does_not_claim_reads_writes_or_flow".to_string(),
            "assignment_targets_without_declaration_are_not_local_bindings".to_string(),
        ],
        MicroNodeKind::AssignmentSite => vec![
            "does_not_claim_local_writes".to_string(),
            "does_not_claim_target_binding".to_string(),
            "does_not_claim_value_flow".to_string(),
            "does_not_claim_mutation_semantics".to_string(),
        ],
        MicroNodeKind::ReturnSite => vec![
            "does_not_claim_returned_value_binding".to_string(),
            "does_not_claim_returns_to_edge".to_string(),
            "does_not_claim_complete_control_flow".to_string(),
            "does_not_claim_runtime_result".to_string(),
        ],
        MicroNodeKind::CallSite => vec![
            "does_not_claim_exact_callee_target".to_string(),
            "does_not_claim_local_calls_edge".to_string(),
            "does_not_claim_runtime_dispatch".to_string(),
            "does_not_claim_function_existence".to_string(),
        ],
        MicroNodeKind::PropertyAccess => vec![
            "does_not_claim_object_type".to_string(),
            "does_not_claim_target_entity".to_string(),
            "does_not_claim_field_declaration".to_string(),
            "does_not_claim_runtime_property".to_string(),
            "does_not_claim_route_auth_sanitizer_or_security_semantics".to_string(),
            "does_not_claim_exact_member_binding_without_symbol_binding_id".to_string(),
            "computed_property_name_is_unknown_unless_literal_contract_supports_it".to_string(),
            "optional_chain_does_not_imply_non_null_behavior".to_string(),
        ],
        MicroNodeKind::ValueUse => vec![
            "does_not_claim_resolver_binding_identity_without_symbol_binding_id".to_string(),
            "does_not_claim_reads_writes_or_flow".to_string(),
            "syntactic_read_occurrence_only".to_string(),
            "does_not_claim_runtime_value".to_string(),
        ],
        _ => vec!["outside_mvp4_1_core_declaration_candidate_slice".to_string()],
    }
}

fn sort_mvp4_ts_candidates(candidates: &mut [Mvp4TypeScriptMicroNodeCandidate]) {
    candidates.sort_by(|left, right| {
        left.repo_relative_path
            .cmp(&right.repo_relative_path)
            .then_with(|| {
                left.enclosing_function_identity
                    .cmp(&right.enclosing_function_identity)
            })
            .then_with(|| left.scope_path.cmp(&right.scope_path))
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
            .then_with(|| left.node_kind.cmp(&right.node_kind))
            .then_with(|| left.structural_ast_path.cmp(&right.structural_ast_path))
            .then_with(|| left.occurrence_index.cmp(&right.occurrence_index))
            .then_with(|| left.name_or_literal.cmp(&right.name_or_literal))
            .then_with(|| left.micro_node_id.cmp(&right.micro_node_id))
    });
}

fn apply_mvp4_ts_micro_node_caps(
    report: &mut Mvp4TypeScriptInMemoryInventoryReport,
    candidates: Vec<Mvp4TypeScriptMicroNodeCandidate>,
    policy: &Mvp4TypeScriptMicroNodeCapPolicy,
) {
    let mut file_count = 0usize;
    let mut function_counts = BTreeMap::<String, usize>::new();
    let mut per_kind_counts = BTreeMap::<(String, MicroNodeKind), usize>::new();
    let mut recovery_count = 0usize;

    for candidate in candidates {
        let function_count = *function_counts
            .get(&candidate.enclosing_function_identity)
            .unwrap_or(&0);
        let kind_key = (
            candidate.enclosing_function_identity.clone(),
            candidate.node_kind,
        );
        let per_kind_count = *per_kind_counts.get(&kind_key).unwrap_or(&0);
        let per_kind_cap = policy
            .per_kind_nodes_per_function
            .get(&candidate.node_kind)
            .copied()
            .unwrap_or(usize::MAX);

        let cap_hit = if file_count >= policy.max_micro_nodes_per_file {
            Some((
                "file_total".to_string(),
                None,
                None,
                file_count,
                format!(
                    "max_micro_nodes_per_file={}",
                    policy.max_micro_nodes_per_file
                ),
            ))
        } else if function_count >= policy.max_micro_nodes_per_function {
            Some((
                "function_total".to_string(),
                Some(candidate.enclosing_function_identity.clone()),
                None,
                function_count,
                format!(
                    "max_micro_nodes_per_function={}",
                    policy.max_micro_nodes_per_function
                ),
            ))
        } else if per_kind_count >= per_kind_cap {
            Some((
                "per_kind_per_function".to_string(),
                Some(candidate.enclosing_function_identity.clone()),
                Some(candidate.node_kind),
                per_kind_count,
                format!(
                    "max_{}_per_function={per_kind_cap}",
                    candidate.node_kind.as_str()
                ),
            ))
        } else if candidate.parse_recovery
            && recovery_count >= policy.max_parser_recovery_derived_nodes
        {
            Some((
                "parser_recovery".to_string(),
                Some(candidate.enclosing_function_identity.clone()),
                Some(candidate.node_kind),
                recovery_count,
                format!(
                    "max_parser_recovery_derived_nodes={}",
                    policy.max_parser_recovery_derived_nodes
                ),
            ))
        } else {
            None
        };

        if let Some((cap_kind, function_identity, node_kind, retained_count, reason)) = cap_hit {
            report.omitted_count += 1;
            record_mvp4_ts_cap_hit(
                report,
                &cap_kind,
                function_identity,
                node_kind,
                retained_count,
                &reason,
                policy.max_omission_details_in_compact_status,
            );
            continue;
        }

        file_count += 1;
        *function_counts
            .entry(candidate.enclosing_function_identity.clone())
            .or_insert(0) += 1;
        *per_kind_counts
            .entry((
                candidate.enclosing_function_identity.clone(),
                candidate.node_kind,
            ))
            .or_insert(0) += 1;
        if candidate.parse_recovery {
            recovery_count += 1;
        }
        report.candidates.push(candidate);
    }

    if report.omitted_count > 0 {
        report.completeness_label = "truncated_unknown_completeness".to_string();
    }
}

fn record_mvp4_ts_cap_hit(
    report: &mut Mvp4TypeScriptInMemoryInventoryReport,
    cap_kind: &str,
    function_identity: Option<String>,
    node_kind: Option<MicroNodeKind>,
    retained_count: usize,
    reason: &str,
    max_details: usize,
) {
    if let Some(existing) = report.cap_hits.iter_mut().find(|hit| {
        hit.cap_kind == cap_kind
            && hit.function_identity == function_identity
            && hit.node_kind == node_kind
            && hit.reason == reason
    }) {
        existing.omitted_count += 1;
        return;
    }
    if report.cap_hits.len() >= max_details {
        return;
    }
    report.cap_hits.push(Mvp4TypeScriptMicroNodeCapHit {
        cap_kind: cap_kind.to_string(),
        function_identity,
        node_kind,
        retained_count,
        omitted_count: 1,
        reason: reason.to_string(),
    });
}

fn finalize_mvp4_ts_inventory_counts(report: &mut Mvp4TypeScriptInMemoryInventoryReport) {
    let mut ids = BTreeMap::<String, String>::new();
    for candidate in &report.candidates {
        let fingerprint = mvp4_ts_candidate_fingerprint(candidate);
        if let Some(existing) = ids.insert(candidate.micro_node_id.clone(), fingerprint.clone()) {
            report.duplicate_candidate_count += 1;
            if existing != fingerprint {
                report.stable_id_collision_count += 1;
            }
        }
        if !mvp4_source_span_is_bounded(&candidate.source_span) {
            report.source_span_failures += 1;
            if candidate.claimability.starts_with("claimable_") {
                report.claimable_node_missing_span_count += 1;
            }
        }
        if mvp4_ts_candidate_overclaims_binding(candidate) {
            report.binding_overclaim_count += 1;
        }
        if mvp4_ts_candidate_overclaims_semantics(candidate) {
            report.semantic_overclaim_count += 1;
        }
    }
}

fn mvp4_ts_candidate_fingerprint(candidate: &Mvp4TypeScriptMicroNodeCandidate) -> String {
    format!(
        "{:?}|{}|{:?}|{}|{:?}|{:?}|{}",
        candidate.node_kind,
        candidate.repo_relative_path,
        candidate.source_span,
        candidate.name_or_literal,
        candidate.scope_path,
        candidate.structural_ast_path,
        candidate.occurrence_index
    )
}

fn mvp4_ts_candidate_overclaims_binding(candidate: &Mvp4TypeScriptMicroNodeCandidate) -> bool {
    matches!(
        candidate.node_kind,
        MicroNodeKind::Parameter
            | MicroNodeKind::LocalBinding
            | MicroNodeKind::AssignmentSite
            | MicroNodeKind::ReturnSite
            | MicroNodeKind::CallSite
            | MicroNodeKind::PropertyAccess
    ) && candidate.symbol_binding_id.is_some()
}

fn mvp4_ts_candidate_overclaims_semantics(candidate: &Mvp4TypeScriptMicroNodeCandidate) -> bool {
    if candidate.node_kind != MicroNodeKind::PropertyAccess {
        return false;
    }
    !candidate
        .provenance
        .limitations
        .contains(&"does_not_claim_route_auth_sanitizer_or_security_semantics".to_string())
        || !candidate
            .provenance
            .limitations
            .contains(&"does_not_claim_runtime_property".to_string())
}

fn mvp4_ts_bound_name_or_literal(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    if max_bytes == 0 {
        return String::new();
    }
    let suffix = format!("...#{}", compact_identity_hash(value));
    if suffix.len() >= max_bytes {
        return suffix.chars().take(max_bytes).collect();
    }
    let mut prefix_budget = max_bytes - suffix.len();
    while !value.is_char_boundary(prefix_budget) {
        prefix_budget -= 1;
    }
    format!("{}{}", &value[..prefix_budget], suffix)
}

fn collect_mvp4_ts_unsupported_core_declaration_boundaries(
    parsed: &ParsedFile,
    source: &str,
) -> Vec<Mvp4TypeScriptUnsupportedDeclarationBoundary> {
    if parsed.language != SourceLanguage::TypeScript
        || !is_mvp4_exact_typescript_ts_path(&parsed.repo_relative_path)
        || mvp4_typescript_source_role(&parsed.repo_relative_path).role
            != MicroSourceRole::Production
    {
        return Vec::new();
    }

    let mut boundaries = Vec::new();
    let mut scope_path = vec![
        normalize_repo_relative_path(&parsed.repo_relative_path),
        MVP4_TS_MICRO_NODE_LANGUAGE.to_string(),
        "unsupported_core_declaration".to_string(),
    ];
    collect_mvp4_ts_unsupported_core_declaration_boundaries_inner(
        parsed.tree().root_node(),
        parsed,
        source,
        &mut scope_path,
        &mut boundaries,
    );
    boundaries.sort_by(|left, right| {
        left.source_span
            .start_line
            .cmp(&right.source_span.start_line)
            .then_with(|| {
                left.source_span
                    .start_column
                    .cmp(&right.source_span.start_column)
            })
            .then_with(|| left.boundary_kind.cmp(&right.boundary_kind))
    });
    boundaries
}

fn collect_mvp4_ts_unsupported_core_declaration_boundaries_inner(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    scope_path: &mut Vec<String>,
    boundaries: &mut Vec<Mvp4TypeScriptUnsupportedDeclarationBoundary>,
) {
    if node.is_error() || node.is_missing() {
        return;
    }

    match node.kind() {
        "formal_parameters" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if mvp4_ts_parameter_binding_node(child).is_none() {
                    if let Some(pattern) = mvp4_ts_parameter_pattern_node(child) {
                        if mvp4_ts_unsupported_binding_pattern(pattern) {
                            boundaries.push(mvp4_ts_unsupported_boundary(
                                parsed,
                                "unsupported_parameter_binding",
                                "parameter pattern is not a simple supported identifier",
                                pattern,
                                scope_path,
                            ));
                        }
                    }
                }
            }
        }
        "variable_declarator" => {
            if let Some(pattern) = node.child_by_field_name("name") {
                if pattern.kind() != "identifier" {
                    boundaries.push(mvp4_ts_unsupported_boundary(
                        parsed,
                        "unsupported_local_binding",
                        "local declaration pattern is not a simple supported identifier",
                        pattern,
                        scope_path,
                    ));
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for (index, child) in node.named_children(&mut cursor).enumerate() {
        scope_path.push(format!("{}#{index}", child.kind()));
        collect_mvp4_ts_unsupported_core_declaration_boundaries_inner(
            child, parsed, source, scope_path, boundaries,
        );
        scope_path.pop();
    }
}

fn mvp4_ts_unsupported_boundary(
    parsed: &ParsedFile,
    boundary_kind: &str,
    reason: &str,
    node: Node<'_>,
    scope_path: &[String],
) -> Mvp4TypeScriptUnsupportedDeclarationBoundary {
    Mvp4TypeScriptUnsupportedDeclarationBoundary {
        boundary_kind: boundary_kind.to_string(),
        reason: reason.to_string(),
        source_span: source_span_for_node(&parsed.repo_relative_path, node),
        scope_path: scope_path.to_vec(),
        structural_ast_path: structural_path_to_node(parsed.tree().root_node(), node),
        exactness: MicroExactness::Unsupported,
        claimability: "non_claimable_unsupported_binding_pattern".to_string(),
    }
}

fn mvp4_ts_parameter_pattern_node(node: Node<'_>) -> Option<Node<'_>> {
    match node.kind() {
        "identifier" => Some(node),
        "required_parameter" | "optional_parameter" => node
            .child_by_field_name("pattern")
            .or_else(|| node.child_by_field_name("name")),
        "rest_pattern" | "object_pattern" | "array_pattern" => Some(node),
        _ => None,
    }
}

fn mvp4_ts_unsupported_binding_pattern(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "object_pattern" | "array_pattern" | "rest_pattern" | "assignment_pattern"
    )
}

fn mvp4_source_span_is_bounded(span: &SourceSpan) -> bool {
    span.start_line > 0
        && span.end_line >= span.start_line
        && span
            .start_column
            .zip(span.end_column)
            .is_none_or(|(start, end)| span.end_line > span.start_line || end >= start)
        && !span.repo_relative_path.trim().is_empty()
}

#[derive(Debug, Clone, Copy)]
struct Mvp4SourceRoleDecision {
    role: MicroSourceRole,
    reason: &'static str,
    source: &'static str,
}

fn is_mvp4_exact_typescript_ts_path(path: &str) -> bool {
    let normalized = normalize_repo_relative_path(path).to_ascii_lowercase();
    normalized.ends_with(".ts") && !normalized.ends_with(".d.ts")
}

fn mvp4_typescript_source_role(path: &str) -> Mvp4SourceRoleDecision {
    let normalized = normalize_repo_relative_path(path).to_ascii_lowercase();
    let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
    if normalized.starts_with("generated/")
        || normalized.starts_with("gen/")
        || normalized.contains("/generated/")
        || normalized.contains("/gen/")
        || file_name.contains(".generated.")
        || file_name.ends_with(".generated.ts")
        || file_name.ends_with(".gen.ts")
        || file_name.ends_with(".d.ts")
    {
        return Mvp4SourceRoleDecision {
            role: MicroSourceRole::Generated,
            reason: "file path is generated source",
            source: "file_path",
        };
    }
    if path_has_component(
        &normalized,
        &["vendor", "vendored", "third_party", "node_modules"],
    ) {
        return Mvp4SourceRoleDecision {
            role: MicroSourceRole::Unknown,
            reason: "file path is vendored/external source",
            source: "file_path",
        };
    }
    if is_test_file_path(path) {
        return Mvp4SourceRoleDecision {
            role: MicroSourceRole::Test,
            reason: "file path is a test/spec path",
            source: "file_path",
        };
    }
    if normalized.starts_with("__mocks__/")
        || normalized.starts_with("mocks/")
        || normalized.starts_with("mock/")
        || normalized.starts_with("stubs/")
        || normalized.starts_with("stub/")
        || normalized.contains("/__mocks__/")
        || normalized.contains("/mocks/")
        || normalized.contains("/mock/")
        || normalized.contains("/stubs/")
        || normalized.contains("/stub/")
        || file_name.contains(".mock.")
        || file_name.contains(".stub.")
        || looks_like_mock_or_stub(file_name)
    {
        return Mvp4SourceRoleDecision {
            role: MicroSourceRole::Mock,
            reason: "file path or file name looks like mock/stub source",
            source: "file_path",
        };
    }
    Mvp4SourceRoleDecision {
        role: MicroSourceRole::Production,
        reason: "default production source for exact .ts file",
        source: "mvp4_first_slice_path_filter",
    }
}

fn collect_mvp4_ts_function_scopes(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    entities: &[Entity],
    class_stack: &mut Vec<String>,
    function_stack: &mut Vec<String>,
    functions: &mut Vec<Mvp4TypeScriptFunctionScope>,
) {
    if node.is_error() || node.is_missing() {
        return;
    }

    if node.kind() == "class_declaration" {
        if let Some(name) = name_from_field_or_child(node, source) {
            class_stack.push(name);
            visit_mvp4_ts_children(
                node,
                parsed,
                source,
                entities,
                class_stack,
                function_stack,
                functions,
            );
            class_stack.pop();
            return;
        }
    }

    if let Some(function) =
        build_mvp4_ts_function_scope(node, parsed, source, entities, class_stack, function_stack)
    {
        let next_name = function.function_name.clone();
        functions.push(function);
        function_stack.push(next_name);
        visit_mvp4_ts_children(
            node,
            parsed,
            source,
            entities,
            class_stack,
            function_stack,
            functions,
        );
        function_stack.pop();
        return;
    }

    visit_mvp4_ts_children(
        node,
        parsed,
        source,
        entities,
        class_stack,
        function_stack,
        functions,
    );
}

fn visit_mvp4_ts_children(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    entities: &[Entity],
    class_stack: &mut Vec<String>,
    function_stack: &mut Vec<String>,
    functions: &mut Vec<Mvp4TypeScriptFunctionScope>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_mvp4_ts_function_scopes(
            child,
            parsed,
            source,
            entities,
            class_stack,
            function_stack,
            functions,
        );
    }
}

fn build_mvp4_ts_function_scope(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    entities: &[Entity],
    class_stack: &[String],
    function_stack: &[String],
) -> Option<Mvp4TypeScriptFunctionScope> {
    let function_kind = match node.kind() {
        "function_declaration" => "function_declaration",
        "generator_function_declaration" => "generator_function_declaration",
        "method_definition" if !class_stack.is_empty() => "method_definition",
        _ => return None,
    };
    let body = node.child_by_field_name("body")?;
    if body.is_error() || body.is_missing() {
        return None;
    }
    let function_name = name_from_field_or_child(node, source)?;
    if function_kind == "method_definition" && function_name == "constructor" {
        return None;
    }

    let source_span = source_span_for_node(&parsed.repo_relative_path, node);
    let body_span = source_span_for_node(&parsed.repo_relative_path, body);
    let structural_ast_path = structural_path_to_node(parsed.tree().root_node(), node);
    let qualifier_path = class_stack
        .iter()
        .chain(function_stack.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut scope_path = vec![
        normalize_repo_relative_path(&parsed.repo_relative_path),
        "typescript".to_string(),
    ];
    scope_path.extend(qualifier_path.iter().map(|value| format!("scope:{value}")));
    scope_path.push(format!("{function_kind}:{function_name}"));
    scope_path.push(format!("ast:{}", structural_ast_path.join("/")));

    let parse_recovery = parsed.has_syntax_errors() || node_has_error_or_missing_descendant(node);
    let parameter_scope = collect_mvp4_ts_parameter_scope(
        node,
        parsed,
        source,
        &scope_path,
        parse_recovery,
        &structural_ast_path,
    );
    let lexical_scopes = collect_mvp4_ts_lexical_scopes(
        body,
        parsed,
        source,
        &scope_path,
        parse_recovery,
        &structural_ast_path,
    );
    let existing_entity_link =
        exact_mvp4_ts_function_entity_link(entities, &function_name, function_kind, &source_span);
    let qualifier_identity = qualifier_path.join("::");
    let structural_identity = structural_ast_path.join("/");
    let function_identity = stable_fact_identity_key(
        "mvp4_ts_function_scope",
        [
            parsed.repo_relative_path.as_str(),
            function_kind,
            function_name.as_str(),
            qualifier_identity.as_str(),
            structural_identity.as_str(),
            MicroSourceRole::Production.as_str(),
        ],
    );

    Some(Mvp4TypeScriptFunctionScope {
        function_identity,
        function_name,
        function_kind: function_kind.to_string(),
        qualifier_path,
        source_span,
        body_span,
        source_role: MicroSourceRole::Production,
        parse_recovery,
        claimability: if parse_recovery {
            "non_claimable_recovery_scope".to_string()
        } else {
            "claimable_source_spanned_scope_existence".to_string()
        },
        scope_path,
        structural_ast_path,
        parameter_scope,
        lexical_scopes,
        existing_entity_link,
    })
}

fn collect_mvp4_ts_parameter_scope(
    function_node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    function_scope_path: &[String],
    function_recovery: bool,
    function_structural_path: &[String],
) -> Mvp4TypeScriptLexicalScope {
    let parameter_node = function_node.child_by_field_name("parameters");
    let source_span = parameter_node
        .map(|node| source_span_for_node(&parsed.repo_relative_path, node))
        .unwrap_or_else(|| source_span_for_node(&parsed.repo_relative_path, function_node));
    let mut scope_path = function_scope_path.to_vec();
    scope_path.push("parameters".to_string());
    let mut bindings = Vec::new();
    if let Some(parameters) = parameter_node {
        let mut cursor = parameters.walk();
        for child in parameters.named_children(&mut cursor) {
            if let Some(binding_node) = mvp4_ts_parameter_binding_node(child) {
                if let Some(name) = node_text(binding_node, source) {
                    bindings.push(Mvp4TypeScriptScopeBinding {
                        name,
                        binding_kind: "parameter".to_string(),
                        declaration_kind: Some("formal_parameter".to_string()),
                        source_span: source_span_for_node(&parsed.repo_relative_path, binding_node),
                        scope_path: scope_path.clone(),
                        structural_ast_path: function_structural_path
                            .iter()
                            .cloned()
                            .chain([
                                "parameters".to_string(),
                                format!(
                                    "parameter:{}",
                                    node_text(binding_node, source).unwrap_or_default()
                                ),
                            ])
                            .collect(),
                        parse_recovery: function_recovery
                            || node_has_error_or_missing_descendant(binding_node),
                    });
                }
            }
        }
    }
    Mvp4TypeScriptLexicalScope {
        scope_kind: "parameter_scope".to_string(),
        scope_path,
        source_span,
        structural_ast_path: function_structural_path
            .iter()
            .cloned()
            .chain(["parameters".to_string()])
            .collect(),
        parse_recovery: function_recovery,
        bindings,
    }
}

fn mvp4_ts_parameter_binding_node(node: Node<'_>) -> Option<Node<'_>> {
    match node.kind() {
        "identifier" => Some(node),
        "required_parameter" | "optional_parameter" => node
            .child_by_field_name("pattern")
            .or_else(|| node.child_by_field_name("name"))
            .filter(|child| child.kind() == "identifier"),
        _ => None,
    }
}

fn mvp4_ts_variable_declaration_kind(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "lexical_declaration" => {
            let text = node_text(node, source)?;
            let trimmed = text.trim_start();
            if trimmed.starts_with("const") {
                Some("const".to_string())
            } else if trimmed.starts_with("let") {
                Some("let".to_string())
            } else {
                Some("lexical".to_string())
            }
        }
        "variable_declaration" => Some("var".to_string()),
        _ => None,
    }
}

fn collect_mvp4_ts_lexical_scopes(
    body: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    function_scope_path: &[String],
    function_recovery: bool,
    function_structural_path: &[String],
) -> Vec<Mvp4TypeScriptLexicalScope> {
    let mut scopes = Vec::new();
    let mut body_scope_path = function_scope_path.to_vec();
    body_scope_path.push("body".to_string());
    collect_mvp4_ts_block_scope(
        body,
        parsed,
        source,
        body_scope_path,
        function_recovery,
        function_structural_path
            .iter()
            .cloned()
            .chain(["body".to_string()])
            .collect(),
        "function_body",
        &mut scopes,
    );
    scopes
}

fn collect_mvp4_ts_block_scope(
    scope_node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    scope_path: Vec<String>,
    inherited_recovery: bool,
    structural_ast_path: Vec<String>,
    scope_kind: &str,
    scopes: &mut Vec<Mvp4TypeScriptLexicalScope>,
) {
    let parse_recovery = inherited_recovery || node_has_error_or_missing_descendant(scope_node);
    let mut scope = Mvp4TypeScriptLexicalScope {
        scope_kind: scope_kind.to_string(),
        scope_path: scope_path.clone(),
        source_span: source_span_for_node(&parsed.repo_relative_path, scope_node),
        structural_ast_path: structural_ast_path.clone(),
        parse_recovery,
        bindings: Vec::new(),
    };
    let mut nested_scopes = Vec::new();
    collect_mvp4_ts_scope_contents(
        scope_node,
        parsed,
        source,
        &scope_path,
        &structural_ast_path,
        parse_recovery,
        &mut scope.bindings,
        &mut nested_scopes,
    );
    scopes.push(scope);
    scopes.extend(nested_scopes);
}

fn collect_mvp4_ts_scope_contents(
    node: Node<'_>,
    parsed: &ParsedFile,
    source: &str,
    scope_path: &[String],
    structural_ast_path: &[String],
    parse_recovery: bool,
    bindings: &mut Vec<Mvp4TypeScriptScopeBinding>,
    nested_scopes: &mut Vec<Mvp4TypeScriptLexicalScope>,
) {
    if node.is_error() || node.is_missing() || is_mvp4_ts_function_scope_node(node) {
        return;
    }
    let mut cursor = node.walk();
    for (index, child) in node.named_children(&mut cursor).enumerate() {
        if is_mvp4_ts_function_scope_node(child) {
            continue;
        }
        if child.kind() == "statement_block" && child.id() != node.id() {
            let mut nested_path = scope_path.to_vec();
            nested_path.push(format!("block:{index}"));
            let nested_structural_path = structural_ast_path
                .iter()
                .cloned()
                .chain([format!("{}#{index}", child.kind())])
                .collect::<Vec<_>>();
            collect_mvp4_ts_block_scope(
                child,
                parsed,
                source,
                nested_path,
                parse_recovery,
                nested_structural_path,
                "block",
                nested_scopes,
            );
            continue;
        }
        if child.kind() == "variable_declarator" {
            if let Some(binding_node) = child.child_by_field_name("name") {
                if binding_node.kind() == "identifier" {
                    if let Some(name) = node_text(binding_node, source) {
                        let declaration_kind = mvp4_ts_variable_declaration_kind(node, source);
                        bindings.push(Mvp4TypeScriptScopeBinding {
                            name,
                            binding_kind: "local".to_string(),
                            declaration_kind,
                            source_span: source_span_for_node(
                                &parsed.repo_relative_path,
                                binding_node,
                            ),
                            scope_path: scope_path.to_vec(),
                            structural_ast_path: structural_ast_path
                                .iter()
                                .cloned()
                                .chain([
                                    "local_declaration".to_string(),
                                    format!(
                                        "binding:{}",
                                        node_text(binding_node, source).unwrap_or_default()
                                    ),
                                ])
                                .collect(),
                            parse_recovery: parse_recovery
                                || node_has_error_or_missing_descendant(binding_node),
                        });
                    }
                }
            }
            continue;
        }
        collect_mvp4_ts_scope_contents(
            child,
            parsed,
            source,
            scope_path,
            structural_ast_path,
            parse_recovery || node_has_error_or_missing_descendant(child),
            bindings,
            nested_scopes,
        );
    }
}

fn is_mvp4_ts_function_scope_node(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "function"
            | "function_expression"
            | "arrow_function"
    )
}

fn exact_mvp4_ts_function_entity_link(
    entities: &[Entity],
    function_name: &str,
    function_kind: &str,
    source_span: &SourceSpan,
) -> Option<Mvp4TypeScriptEntityLink> {
    let expected_kind = match function_kind {
        "method_definition" => EntityKind::Method,
        _ => EntityKind::Function,
    };
    entities
        .iter()
        .find(|entity| {
            entity.kind == expected_kind
                && entity.name == function_name
                && entity.source_span.as_ref() == Some(source_span)
        })
        .map(|entity| Mvp4TypeScriptEntityLink {
            entity_id: entity.id.clone(),
            entity_kind: entity.kind,
            exactness: "exact_existing_parser_entity_source_span_match".to_string(),
        })
}

fn structural_path_to_node(root: Node<'_>, target: Node<'_>) -> Vec<String> {
    let mut path = vec![format!("{}#0", root.kind())];
    if structural_path_to_node_inner(root, target.id(), &mut path) {
        path
    } else {
        vec![format!("{}#unresolved", target.kind())]
    }
}

fn structural_path_to_node_inner(node: Node<'_>, target_id: usize, path: &mut Vec<String>) -> bool {
    if node.id() == target_id {
        return true;
    }
    let mut cursor = node.walk();
    for (index, child) in node.named_children(&mut cursor).enumerate() {
        path.push(format!("{}#{index}", child.kind()));
        if structural_path_to_node_inner(child, target_id, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn parser_file_metadata(parsed: &ParsedFile) -> Metadata {
    let mut metadata = Metadata::new();
    metadata.insert(
        "parser_frontend".to_string(),
        parsed.language.as_str().into(),
    );
    metadata.insert(
        "source_span_quality".to_string(),
        "tree_sitter_byte_and_point_spans".into(),
    );
    metadata.insert(
        "parser_error_handling".to_string(),
        "tree_sitter_error_and_missing_nodes_reported".into(),
    );
    metadata.insert(
        "syntax_error_count".to_string(),
        parsed.diagnostics.len().into(),
    );
    if parsed.has_syntax_errors() {
        metadata.insert(
            "parser_status".to_string(),
            "syntax_errors_recovered".into(),
        );
        metadata.insert("claim_state".to_string(), "partial".into());
        metadata.insert(
            "unsupported_behavior_label".to_string(),
            "malformed_regions_not_trusted_for_exact_relations".into(),
        );
        metadata.insert(
            "parse_diagnostics".to_string(),
            json!(parsed
                .diagnostics
                .iter()
                .take(16)
                .map(|diagnostic| {
                    json!({
                        "message": diagnostic.message.clone(),
                        "kind": diagnostic.node.kind.clone(),
                        "is_error": diagnostic.node.is_error,
                        "is_missing": diagnostic.node.is_missing,
                        "span": diagnostic.node.source_span.to_string(),
                    })
                })
                .collect::<Vec<_>>()),
        );
    } else {
        metadata.insert("parser_status".to_string(), "parsed".into());
        metadata.insert("claim_state".to_string(), "exact".into());
    }
    metadata
}

struct BasicEntityExtractor<'a> {
    parsed: &'a ParsedFile,
    source: &'a str,
    file_hash: String,
    entities: Vec<Entity>,
    entity_indices: BTreeMap<String, usize>,
    edges: Vec<Edge>,
    entity_kinds: BTreeMap<String, EntityKind>,
    entity_source_roles: BTreeMap<String, EvidenceRole>,
    edge_ids: BTreeSet<String>,
    scope_parents: BTreeMap<String, String>,
    symbols_by_scope: BTreeMap<String, BTreeMap<String, SymbolRef>>,
    ambiguous_symbols_by_scope: BTreeMap<String, BTreeSet<String>>,
    table_symbols_by_scope: BTreeMap<String, BTreeMap<String, String>>,
    parameters_by_scope: BTreeMap<String, Vec<String>>,
    return_sites_by_scope: BTreeMap<String, Vec<String>>,
    test_file_id: Option<String>,
}

#[derive(Debug, Clone)]
struct SymbolRef {
    id: String,
    exactness: Exactness,
    confidence: f64,
}

#[derive(Debug, Clone)]
struct ImportBinding {
    local_name: String,
    imported_name: Option<String>,
    module_specifier: Option<String>,
    import_kind: String,
    claim_state: String,
    syntax_claim_state: String,
    target_resolution_claim_state: String,
    resolution: String,
    unsupported_reason: Option<String>,
}

impl ImportBinding {
    fn parser_observed(
        local_name: impl Into<String>,
        imported_name: Option<String>,
        module_specifier: Option<String>,
        import_kind: impl Into<String>,
    ) -> Self {
        Self {
            local_name: local_name.into(),
            imported_name,
            module_specifier,
            import_kind: import_kind.into(),
            claim_state: "partial".to_string(),
            syntax_claim_state: "exact".to_string(),
            target_resolution_claim_state: "unresolved".to_string(),
            resolution: "parser_observed_unresolved_static_import".to_string(),
            unsupported_reason: None,
        }
    }

    fn target_unsupported(
        local_name: impl Into<String>,
        imported_name: Option<String>,
        module_specifier: Option<String>,
        import_kind: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            local_name: local_name.into(),
            imported_name,
            module_specifier,
            import_kind: import_kind.into(),
            claim_state: "partial".to_string(),
            syntax_claim_state: "exact".to_string(),
            target_resolution_claim_state: "unsupported".to_string(),
            resolution: "parser_observed_import_target_unsupported".to_string(),
            unsupported_reason: Some(reason.into()),
        }
    }

    fn preprocessor_include(
        local_name: impl Into<String>,
        module_specifier: Option<String>,
        import_kind: impl Into<String>,
    ) -> Self {
        Self {
            local_name: local_name.into(),
            imported_name: None,
            module_specifier,
            import_kind: import_kind.into(),
            claim_state: "unsupported".to_string(),
            syntax_claim_state: "exact".to_string(),
            target_resolution_claim_state: "unsupported".to_string(),
            resolution: "unresolved_preprocessor_include".to_string(),
            unsupported_reason: Some(
                "preprocessor include target resolution requires compiler include paths"
                    .to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct HeuristicTag<'a> {
    pattern: &'a str,
    framework: &'a str,
    confidence: f64,
}

impl<'a> HeuristicTag<'a> {
    const fn new(pattern: &'a str, framework: &'a str, confidence: f64) -> Self {
        Self {
            pattern,
            framework,
            confidence,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct EdgeAnnotation<'a> {
    exactness: Exactness,
    confidence: f64,
    extractor: &'a str,
    heuristic: Option<HeuristicTag<'a>>,
}

struct GenericLanguageExtractor<'a> {
    parsed: &'a ParsedFile,
    source: &'a str,
    file_hash: String,
    entities: Vec<Entity>,
    entity_indices: BTreeMap<String, usize>,
    edges: Vec<Edge>,
    entity_kinds: BTreeMap<String, EntityKind>,
    entity_source_roles: BTreeMap<String, EvidenceRole>,
    edge_ids: BTreeSet<String>,
    scope_parents: BTreeMap<String, String>,
    symbols_by_scope: BTreeMap<String, BTreeMap<String, SymbolRef>>,
    ambiguous_symbols_by_scope: BTreeMap<String, BTreeSet<String>>,
    parameters_by_scope: BTreeMap<String, Vec<String>>,
    return_sites_by_scope: BTreeMap<String, Vec<String>>,
    rust_impl_type_by_scope: BTreeMap<String, String>,
    rust_methods_by_type: BTreeMap<String, BTreeMap<String, SymbolRef>>,
    rust_ambiguous_methods_by_type: BTreeMap<String, BTreeSet<String>>,
    rust_local_types_by_scope: BTreeMap<String, BTreeMap<String, String>>,
    test_file_id: Option<String>,
    test_case_by_scope: BTreeMap<String, String>,
}

impl<'a> GenericLanguageExtractor<'a> {
    fn new(parsed: &'a ParsedFile, source: &'a str) -> Self {
        Self {
            parsed,
            source,
            file_hash: content_hash(source),
            entities: Vec::new(),
            entity_indices: BTreeMap::new(),
            edges: Vec::new(),
            entity_kinds: BTreeMap::new(),
            entity_source_roles: BTreeMap::new(),
            edge_ids: BTreeSet::new(),
            scope_parents: BTreeMap::new(),
            symbols_by_scope: BTreeMap::new(),
            ambiguous_symbols_by_scope: BTreeMap::new(),
            parameters_by_scope: BTreeMap::new(),
            return_sites_by_scope: BTreeMap::new(),
            rust_impl_type_by_scope: BTreeMap::new(),
            rust_methods_by_type: BTreeMap::new(),
            rust_ambiguous_methods_by_type: BTreeMap::new(),
            rust_local_types_by_scope: BTreeMap::new(),
            test_file_id: None,
            test_case_by_scope: BTreeMap::new(),
        }
    }

    fn extract(mut self) -> BasicExtraction {
        let file_record = FileRecord {
            repo_relative_path: self.parsed.repo_relative_path.clone(),
            file_hash: self.file_hash.clone(),
            language: Some(self.parsed.language.to_string()),
            size_bytes: self.parsed.byte_len as u64,
            indexed_at_unix_ms: None,
            metadata: parser_file_metadata(self.parsed),
        };

        let file_name = self.parsed.repo_relative_path.clone();
        let file_symbol_name = module_name_for_path(&self.parsed.repo_relative_path);
        let file_span = self.parsed.root_node.source_span.clone();
        let file_id = self.push_entity(
            EntityKind::File,
            &file_symbol_name,
            &file_name,
            file_span.clone(),
        );
        if is_test_file_path(&self.parsed.repo_relative_path) {
            let test_file_id = self.push_entity(
                EntityKind::TestFile,
                &file_name,
                &format!("test::{file_name}"),
                file_span.clone(),
            );
            self.push_edge(&file_id, RelationKind::Contains, &test_file_id, &file_span);
            self.push_edge(&test_file_id, RelationKind::DefinedIn, &file_id, &file_span);
            self.test_file_id = Some(test_file_id);
        }
        let module_name = module_name_for_path(&self.parsed.repo_relative_path);
        let module_id = self.push_entity(
            EntityKind::Module,
            &module_name,
            &module_name,
            self.parsed.root_node.source_span.clone(),
        );
        self.scope_parents
            .insert(module_id.clone(), file_id.clone());
        self.push_edge(&file_id, RelationKind::Contains, &module_id, &file_span);
        self.push_edge(&file_id, RelationKind::Defines, &module_id, &file_span);
        self.push_edge(&module_id, RelationKind::DefinedIn, &file_id, &file_span);

        self.visit_children(self.parsed.tree.root_node(), &module_id, &module_name);
        self.recover_syntax_error_declarations(&module_id, &module_name);

        BasicExtraction {
            file: file_record,
            entities: self.entities,
            edges: self.edges,
        }
    }

    fn visit_children(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        self.predeclare_visible_child_symbols(node, scope_id, scope_name);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_node(child, scope_id, scope_name);
        }
    }

    fn predeclare_visible_child_symbols(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
    ) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.is_error() || child.is_missing() {
                continue;
            }
            if let Some((kind, name, qualified_name)) =
                self.scoped_declaration_identity(child, scope_id, scope_name)
            {
                self.push_scoped_entity(scope_id, kind, &name, &qualified_name, child);
                continue;
            }
            self.predeclare_visible_child_symbols(child, scope_id, scope_name);
        }
    }

    fn scoped_declaration_identity(
        &self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
    ) -> Option<(EntityKind, String, String)> {
        let mut kind = generic_decl_kind(self.parsed.language, node)?;
        let name = generic_decl_name(node, self.source)?;
        if self.parsed.language == SourceLanguage::Rust
            && kind == EntityKind::Function
            && rust_function_decl_within_impl_item(node)
        {
            kind = EntityKind::Method;
        } else if kind == EntityKind::Function
            && self.entity_kinds.get(scope_id).is_some_and(|scope_kind| {
                matches!(
                    scope_kind,
                    EntityKind::Class | EntityKind::Interface | EntityKind::Trait
                )
            })
        {
            kind = if matches!(name.as_str(), "__init__" | "constructor" | "new") {
                EntityKind::Constructor
            } else {
                EntityKind::Method
            };
        }
        let qualified_name = qualify(scope_name, &name);
        Some((kind, name, qualified_name))
    }

    fn recover_syntax_error_declarations(&mut self, scope_id: &str, scope_name: &str) {
        if !self.parsed.has_syntax_errors() {
            return;
        }
        for declaration in recoverable_declarations_from_source(
            self.parsed.language,
            &self.parsed.repo_relative_path,
            self.source,
        ) {
            if self.entities.iter().any(|entity| {
                entity.kind == declaration.kind
                    && entity.name == declaration.name
                    && entity.repo_relative_path == self.parsed.repo_relative_path
            }) {
                continue;
            }
            let qualified_name = qualify(scope_name, &declaration.name);
            let id = self.push_entity(
                declaration.kind,
                &declaration.name,
                &qualified_name,
                declaration.span.clone(),
            );
            if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
                entity.created_from = "tree-sitter-syntax-recovery".to_string();
                annotate_untrusted_syntax_entity(
                    entity,
                    "recovered declaration from syntax-error source prefix",
                );
                entity
                    .metadata
                    .insert("syntax_recovery".to_string(), true.into());
            }
            if is_scope_kind(declaration.kind) {
                self.scope_parents.insert(id.clone(), scope_id.to_string());
            }
            self.register_symbol_with(
                scope_id,
                &declaration.name,
                &id,
                Exactness::StaticHeuristic,
                0.45,
            );
            self.push_edge_with(
                scope_id,
                RelationKind::Contains,
                &id,
                &declaration.span,
                Exactness::StaticHeuristic,
                0.45,
            );
            self.push_edge_with(
                &id,
                RelationKind::DefinedIn,
                scope_id,
                &declaration.span,
                Exactness::StaticHeuristic,
                0.45,
            );
            let relation = if matches!(
                declaration.kind,
                EntityKind::Class
                    | EntityKind::Interface
                    | EntityKind::Trait
                    | EntityKind::Enum
                    | EntityKind::Function
                    | EntityKind::Method
                    | EntityKind::Constructor
            ) {
                RelationKind::Defines
            } else {
                RelationKind::Declares
            };
            self.push_edge_with(
                scope_id,
                relation,
                &id,
                &declaration.span,
                Exactness::StaticHeuristic,
                0.45,
            );
        }
    }

    fn visit_node(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        if node.is_error() || node.is_missing() {
            return;
        }
        let node_untrusted = node_has_error_or_missing_descendant(node);

        if let Some((kind, name, qualified_name)) =
            self.scoped_declaration_identity(node, scope_id, scope_name)
        {
            let id = self.push_scoped_entity(scope_id, kind, &name, &qualified_name, node);
            if is_test_case_declaration(
                self.parsed.language,
                &self.parsed.repo_relative_path,
                kind,
                &name,
                &qualified_name,
                node,
                self.source,
            ) {
                let test_id =
                    self.push_test_case_entity(scope_id, &id, &name, &qualified_name, node);
                self.test_case_by_scope.insert(id.clone(), test_id);
            }
            if node_untrusted {
                return;
            }
            self.extract_parameters(node, &id, &qualified_name);
            self.visit_children(node, &id, &qualified_name);
            return;
        }

        if node_untrusted {
            if let Some(name) = generic_local_variable_name(self.parsed.language, node, self.source)
            {
                let qualified_name = qualify(scope_name, &name);
                let id = self.push_scoped_entity(
                    scope_id,
                    EntityKind::LocalVariable,
                    &name,
                    &qualified_name,
                    node,
                );
                if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
                    annotate_untrusted_syntax_entity(
                        entity,
                        "local variable node contains tree-sitter ERROR or MISSING descendant",
                    );
                }
            }
            return;
        }

        if is_generic_import_node(self.parsed.language, node) {
            self.extract_import(scope_id, scope_name, node);
        } else if is_generic_export_node(self.parsed.language, node, self.source) {
            self.extract_export(scope_id, scope_name, node);
        } else if is_generic_assertion_syntax_node(self.parsed.language, node, self.source) {
            self.extract_generic_assertion_syntax(node, scope_id, scope_name);
        } else if self.parsed.language == SourceLanguage::Go && node.kind() == "go_statement" {
            self.extract_go_statement(node, scope_id, scope_name);
        } else if is_generic_call_node(self.parsed.language, node) {
            self.extract_call(node, scope_id, scope_name);
        } else if is_generic_assignment_node(self.parsed.language, node) {
            self.extract_assignment(node, scope_id, scope_name);
        } else if is_generic_return_node(self.parsed.language, node) {
            self.extract_return(node, scope_id, scope_name);
        } else if let Some(name) =
            generic_local_variable_name(self.parsed.language, node, self.source)
        {
            let qualified_name = qualify(scope_name, &name);
            self.push_scoped_entity(
                scope_id,
                EntityKind::LocalVariable,
                &name,
                &qualified_name,
                node,
            );
        }

        self.visit_children(node, scope_id, scope_name);
    }

    fn extract_go_statement(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let Some(call) = go_statement_call_node(node) else {
            let task_name = format!("go@{}", node.start_byte());
            let task_id = self.push_entity(
                EntityKind::Task,
                &task_name,
                &qualify(scope_name, &task_name),
                span.clone(),
            );
            self.push_edge_with(
                scope_id,
                RelationKind::Spawns,
                &task_id,
                &span,
                Exactness::StaticHeuristic,
                0.5,
            );
            return;
        };
        let callee_node = generic_call_callee_node(self.parsed.language, call);
        let (callee_id, exactness, confidence) =
            self.callee_entity(call, callee_node, scope_id, scope_name);
        let exactness = if is_proof_grade_exactness(exactness) {
            exactness
        } else {
            Exactness::StaticHeuristic
        };
        self.push_edge_with(
            scope_id,
            RelationKind::Spawns,
            &callee_id,
            &span,
            exactness,
            confidence,
        );
    }

    fn extract_assignment(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let Some(left) = generic_assignment_target_node(self.parsed.language, node) else {
            return;
        };
        let Some(target) = self.assignment_target_entity(node, left, scope_id, scope_name) else {
            return;
        };
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        self.push_edge_with(
            scope_id,
            RelationKind::Writes,
            &target.id,
            &span,
            target.exactness,
            target.confidence,
        );
        if generic_assignment_target_is_property(left) {
            self.push_edge_with(
                scope_id,
                RelationKind::Mutates,
                &target.id,
                &span,
                Exactness::StaticHeuristic,
                target.confidence.min(0.75),
            );
        }

        if let Some(right) = generic_assignment_value_node(self.parsed.language, node) {
            if let Some(source) = self.expression_entity(right, scope_id, scope_name) {
                self.push_edge_with(
                    &target.id,
                    RelationKind::AssignedFrom,
                    &source.id,
                    &span,
                    source.exactness,
                    source.confidence,
                );
                self.push_edge_with(
                    &source.id,
                    RelationKind::FlowsTo,
                    &target.id,
                    &span,
                    source.exactness,
                    source.confidence,
                );
            }
            let value_span = source_span_for_node(&self.parsed.repo_relative_path, right);
            self.extract_return_value_assignment_flow(
                right,
                &target,
                &value_span,
                scope_id,
                scope_name,
            );
            self.extract_reads_from_expression(right, scope_id, Some(target.id.as_str()));
        }
    }

    fn extract_return(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let return_name = format!("return@{}", node.start_byte());
        let return_id = self.push_entity(
            EntityKind::ReturnSite,
            &return_name,
            &qualify(scope_name, &return_name),
            span.clone(),
        );
        self.push_edge(scope_id, RelationKind::Contains, &return_id, &span);
        self.push_edge(&return_id, RelationKind::DefinedIn, scope_id, &span);
        self.push_edge(scope_id, RelationKind::Returns, &return_id, &span);
        self.push_edge(&return_id, RelationKind::ReturnsTo, scope_id, &span);
        self.return_sites_by_scope
            .entry(scope_id.to_string())
            .or_default()
            .push(return_id.clone());

        if let Some(value) = generic_return_value_node(self.parsed.language, node) {
            if let Some(source) = self.expression_entity(value, scope_id, scope_name) {
                let value_span = source_span_for_node(&self.parsed.repo_relative_path, value);
                self.push_edge_with(
                    &source.id,
                    RelationKind::FlowsTo,
                    &return_id,
                    &value_span,
                    source.exactness,
                    source.confidence,
                );
            }
            self.extract_reads_from_expression(value, scope_id, None);
        }
    }

    fn extract_return_value_assignment_flow(
        &mut self,
        value: Node<'_>,
        target: &SymbolRef,
        span: &SourceSpan,
        scope_id: &str,
        scope_name: &str,
    ) {
        if !is_generic_call_node(self.parsed.language, value)
            || !is_proof_grade_exactness(target.exactness)
        {
            return;
        }
        let callee_node = generic_call_callee_node(self.parsed.language, value);
        let (callee_id, exactness, confidence) =
            self.callee_entity(value, callee_node, scope_id, scope_name);
        if !is_proof_grade_exactness(exactness) {
            return;
        }
        let Some(return_sites) = self.return_sites_by_scope.get(&callee_id).cloned() else {
            return;
        };
        for return_site_id in return_sites {
            self.push_edge_with(
                &return_site_id,
                RelationKind::FlowsTo,
                &target.id,
                span,
                exactness,
                confidence.min(target.confidence),
            );
        }
    }

    fn extract_import(&mut self, scope_id: &str, scope_name: &str, node: Node<'_>) {
        let name = generic_import_name(self.parsed.language, node, self.source)
            .unwrap_or_else(|| statement_label(node, self.source));
        let qualified_name = qualify(scope_name, &format!("import:{name}"));
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity(EntityKind::Import, &name, &qualified_name, span.clone());
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(scope_id, RelationKind::Imports, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
        let statement_binding = statement_import_binding(self.parsed.language, &name);
        annotate_import_artifact(&mut self.entities, &mut self.edges, &id, &statement_binding);

        for (index, binding) in import_bindings_for_node(self.parsed.language, node, self.source)
            .into_iter()
            .enumerate()
        {
            let binding_id = self.push_import_binding(scope_id, scope_name, node, index, &binding);
            annotate_import_artifact(&mut self.entities, &mut self.edges, &binding_id, &binding);
            self.register_parser_observed_import_binding(scope_id, &binding_id, &binding);
            self.register_rust_import_binding(scope_id, &binding);
            self.push_unresolved_import_target_reference(scope_id, scope_name, node, &binding);
        }
    }

    /// Emits the §1.3 unresolved-import-target reference: the import TARGET
    /// (not the local binding) as a reference entity plus a StaticHeuristic
    /// IMPORTS edge, so imports of nonexistent symbols become visible
    /// lane facts the same way unresolved callees do.
    fn push_unresolved_import_target_reference(
        &mut self,
        scope_id: &str,
        scope_name: &str,
        node: Node<'_>,
        binding: &ImportBinding,
    ) {
        let Some(target_label) = unresolved_import_target_label(self.parsed.language, binding)
        else {
            return;
        };
        let target_id = self.push_reference_entity(
            EntityKind::Import,
            &target_label,
            scope_name,
            node,
            "unresolved-import-target",
            0.5,
        );
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        self.push_edge_with(
            scope_id,
            RelationKind::Imports,
            &target_id,
            &span,
            Exactness::StaticHeuristic,
            0.5,
        );
    }

    fn push_import_binding(
        &mut self,
        scope_id: &str,
        scope_name: &str,
        node: Node<'_>,
        index: usize,
        binding: &ImportBinding,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let qualified_name = qualify(
            scope_name,
            &format!(
                "import:{}:{}#{}",
                binding.import_kind, binding.local_name, index
            ),
        );
        let id = self.push_entity(
            EntityKind::Import,
            &binding.local_name,
            &qualified_name,
            span.clone(),
        );
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(scope_id, RelationKind::Imports, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
        id
    }

    fn extract_export(&mut self, scope_id: &str, scope_name: &str, node: Node<'_>) {
        let name = statement_label(node, self.source);
        let qualified_name = qualify(scope_name, &format!("export:{name}"));
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity(EntityKind::Export, &name, &qualified_name, span.clone());
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(scope_id, RelationKind::Exports, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
    }

    fn extract_parameters(&mut self, node: Node<'_>, parent_id: &str, parent_name: &str) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if generic_parameter_container(child.kind()) {
                self.extract_parameters_from_list(child, parent_id, parent_name);
            }
        }
    }

    fn extract_parameters_from_list(&mut self, node: Node<'_>, parent_id: &str, parent_name: &str) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if !generic_parameter_node(child.kind()) {
                continue;
            }
            if let Some(name) = generic_parameter_name(child, self.source) {
                let qualified_name = qualify(parent_name, &format!("param:{name}"));
                let id = self.push_scoped_entity(
                    parent_id,
                    EntityKind::Parameter,
                    &name,
                    &qualified_name,
                    child,
                );
                self.register_rust_parameter_type(parent_id, &name, child);
                self.parameters_by_scope
                    .entry(parent_id.to_string())
                    .or_default()
                    .push(id);
            }
        }
    }

    fn extract_call(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let callee_node = generic_call_callee_node(self.parsed.language, node);
        let callee_label = callee_node
            .and_then(|callee| {
                node_text(callee, self.source).map(|text| compact_extracted_label(&text, callee))
            })
            .unwrap_or_else(|| "unknown_callee".to_string());
        let callsite_name = format!("call:{callee_label}");
        let callsite_id = self.push_entity(
            EntityKind::CallSite,
            &callsite_name,
            &qualify(
                scope_name,
                &format!("{callsite_name}@{}", node.start_byte()),
            ),
            span.clone(),
        );
        self.scope_parents
            .insert(callsite_id.clone(), scope_id.to_string());
        self.push_edge(scope_id, RelationKind::Contains, &callsite_id, &span);
        self.push_edge(&callsite_id, RelationKind::DefinedIn, scope_id, &span);

        let (callee_id, exactness, confidence) =
            self.callee_entity(node, callee_node, scope_id, scope_name);
        self.push_edge_with(
            scope_id,
            RelationKind::Calls,
            &callee_id,
            &span,
            exactness,
            confidence,
        );
        self.push_edge_with(
            &callsite_id,
            RelationKind::Callee,
            &callee_id,
            &span,
            exactness,
            confidence,
        );
        self.extract_obvious_mutation_call(scope_id, &callee_label, &span);
        self.extract_test_relation_for_call(
            node,
            scope_id,
            scope_name,
            &callee_id,
            &callee_label,
            &span,
            exactness,
            confidence,
        );

        if let Some(arguments) = generic_call_arguments_node(self.parsed.language, node) {
            self.extract_call_arguments(arguments, &callsite_id, scope_id, scope_name);
            self.extract_argument_to_parameter_flows(arguments, scope_id, &callee_id);
        }
    }

    fn extract_call_arguments(
        &mut self,
        arguments: Node<'_>,
        callsite_id: &str,
        scope_id: &str,
        scope_name: &str,
    ) {
        let mut cursor = arguments.walk();
        for (index, argument) in arguments.named_children(&mut cursor).enumerate() {
            let span = source_span_for_node(&self.parsed.repo_relative_path, argument);
            let arg_name = format!("argument_{index}@{}", argument.start_byte());
            let arg_id = self.push_entity(
                EntityKind::Expression,
                &arg_name,
                &qualify(scope_name, &format!("{arg_name}@{}", argument.start_byte())),
                span.clone(),
            );
            let relation = match index {
                0 => RelationKind::Argument0,
                1 => RelationKind::Argument1,
                _ => RelationKind::ArgumentN,
            };
            self.push_edge(callsite_id, relation, &arg_id, &span);
            if let Some(name) = deepest_identifier(argument, self.source) {
                if let Some(symbol) = self.resolve_symbol(scope_id, &name) {
                    self.push_edge_with(
                        &symbol.id,
                        RelationKind::FlowsTo,
                        &arg_id,
                        &span,
                        symbol.exactness,
                        symbol.confidence,
                    );
                }
            }
            self.extract_reads_from_expression(argument, scope_id, None);
        }
    }

    fn extract_test_relation_for_call(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
        callee_id: &str,
        callee_label: &str,
        span: &SourceSpan,
        exactness: Exactness,
        confidence: f64,
    ) {
        let Some(test_case_id) = self.test_case_by_scope.get(scope_id).cloned() else {
            return;
        };
        let lower = callee_label.to_ascii_lowercase();
        if is_assert_call(&lower) || is_generic_assert_call(self.parsed.language, &lower) {
            self.extract_generic_assertion(&test_case_id, node, scope_id, scope_name, span);
            return;
        }
        if let Some(relation) = generic_mock_or_stub_relation(&lower) {
            let target_id = self.push_expression_entity(
                callee_label,
                scope_name,
                node,
                "test-double-api-target",
                0.62,
            );
            self.push_edge_with(
                &test_case_id,
                relation,
                &target_id,
                span,
                Exactness::StaticHeuristic,
                0.62,
            );
            return;
        }
        if !is_proof_grade_exactness(exactness)
            || callee_id == scope_id
            || callee_id == test_case_id
        {
            return;
        }
        if self.entity_kinds.get(callee_id).is_some_and(|kind| {
            matches!(
                kind,
                EntityKind::Function | EntityKind::Method | EntityKind::Constructor
            )
        }) {
            self.push_edge_with(
                &test_case_id,
                RelationKind::Tests,
                callee_id,
                span,
                exactness,
                confidence,
            );
        }
    }

    fn extract_generic_assertion(
        &mut self,
        test_case_id: &str,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
        span: &SourceSpan,
    ) {
        let assertion_name = format!("assert@{}", node.start_byte());
        let assertion_id = self.push_entity(
            EntityKind::Assertion,
            &assertion_name,
            &qualify(scope_name, &assertion_name),
            span.clone(),
        );
        self.push_edge(test_case_id, RelationKind::Contains, &assertion_id, span);
        self.push_edge(&assertion_id, RelationKind::DefinedIn, scope_id, span);
        let target_id = generic_call_arguments_node(self.parsed.language, node)
            .and_then(|arguments| {
                first_assertion_target_argument(self.parsed.language, arguments, self.source)
            })
            .and_then(|argument| self.expression_entity(argument, scope_id, scope_name))
            .map(|target| target.id)
            .unwrap_or_else(|| {
                self.push_expression_entity(
                    "assertion-target",
                    scope_name,
                    node,
                    "assertion-target",
                    0.72,
                )
            });
        self.push_edge_with(
            test_case_id,
            RelationKind::Asserts,
            &target_id,
            span,
            Exactness::ParserVerified,
            0.86,
        );
    }

    fn extract_generic_assertion_syntax(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
    ) {
        let Some(test_case_id) = self.test_case_by_scope.get(scope_id).cloned() else {
            return;
        };
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let assertion_name = format!("assert@{}", node.start_byte());
        let assertion_id = self.push_entity(
            EntityKind::Assertion,
            &assertion_name,
            &qualify(scope_name, &assertion_name),
            span.clone(),
        );
        self.push_edge(&test_case_id, RelationKind::Contains, &assertion_id, &span);
        self.push_edge(&assertion_id, RelationKind::DefinedIn, scope_id, &span);
        let target_id = self.push_expression_entity(
            "assertion-target",
            scope_name,
            node,
            "assertion-syntax-target",
            0.76,
        );
        self.push_edge_with(
            &test_case_id,
            RelationKind::Asserts,
            &target_id,
            &span,
            Exactness::ParserVerified,
            0.84,
        );
    }

    fn push_test_case_entity(
        &mut self,
        scope_id: &str,
        function_id: &str,
        name: &str,
        qualified_name: &str,
        node: Node<'_>,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let test_qualified_name = format!("test_case::{qualified_name}");
        let test_id = self.push_entity(
            EntityKind::TestCase,
            name,
            &test_qualified_name,
            span.clone(),
        );
        let container = self
            .test_file_id
            .clone()
            .unwrap_or_else(|| function_id.to_string());
        self.push_edge(&container, RelationKind::Contains, &test_id, &span);
        self.push_edge(&test_id, RelationKind::DefinedIn, &container, &span);
        self.push_edge(function_id, RelationKind::Contains, &test_id, &span);
        self.push_edge(&test_id, RelationKind::DefinedIn, function_id, &span);
        if self.test_file_id.is_none() {
            self.push_edge(scope_id, RelationKind::Contains, &test_id, &span);
        }
        test_id
    }

    fn extract_argument_to_parameter_flows(
        &mut self,
        arguments: Node<'_>,
        scope_id: &str,
        callee_id: &str,
    ) {
        let Some(parameters) = self.parameters_by_scope.get(callee_id).cloned() else {
            return;
        };
        let mut cursor = arguments.walk();
        for (index, argument) in arguments.named_children(&mut cursor).enumerate() {
            let Some(parameter_id) = parameters.get(index) else {
                continue;
            };
            let Some(source) = self.direct_argument_symbol(argument, scope_id) else {
                continue;
            };
            let span = source_span_for_node(&self.parsed.repo_relative_path, argument);
            self.push_edge_with(
                &source.id,
                RelationKind::FlowsTo,
                parameter_id,
                &span,
                source.exactness,
                source.confidence,
            );
        }
    }

    fn direct_argument_symbol(&self, argument: Node<'_>, scope_id: &str) -> Option<SymbolRef> {
        if !generic_assignment_target_is_identifier(argument) {
            return None;
        }
        let name = node_text(argument, self.source)?;
        self.resolve_symbol(scope_id, &name)
    }

    fn extract_obvious_mutation_call(
        &mut self,
        scope_id: &str,
        callee_label: &str,
        span: &SourceSpan,
    ) {
        let Some((receiver, _method)) = simple_mutation_method_receiver(callee_label) else {
            return;
        };
        let Some(symbol) = self.resolve_symbol(scope_id, receiver) else {
            return;
        };
        self.push_edge_with(
            scope_id,
            RelationKind::Mutates,
            &symbol.id,
            span,
            Exactness::StaticHeuristic,
            symbol.confidence.min(0.72),
        );
    }

    fn extract_reads_from_expression(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        skip_id: Option<&str>,
    ) {
        let mut identifiers = Vec::new();
        collect_identifier_nodes_capped(
            node,
            &mut identifiers,
            expression_read_identifier_cap(
                &self.parsed.repo_relative_path,
                self.source.len(),
                node,
            ),
        );
        for identifier in identifiers {
            let Some(name) = node_text(identifier, self.source) else {
                continue;
            };
            let Some(symbol) = self.resolve_or_reference_symbol(scope_id, &name, identifier) else {
                continue;
            };
            if skip_id == Some(symbol.id.as_str()) {
                continue;
            }
            let span = source_span_for_node(&self.parsed.repo_relative_path, identifier);
            self.push_edge_with(
                scope_id,
                RelationKind::Reads,
                &symbol.id,
                &span,
                symbol.exactness,
                symbol.confidence,
            );
        }
    }

    fn assignment_target_entity(
        &mut self,
        assignment: Node<'_>,
        target: Node<'_>,
        scope_id: &str,
        scope_name: &str,
    ) -> Option<SymbolRef> {
        if let Some(name) = generic_assignment_single_identifier_name(target, self.source) {
            if let Some(symbol) = self.resolve_symbol(scope_id, &name) {
                return Some(symbol);
            }
            if generic_assignment_declares_local(self.parsed.language, assignment) {
                let qualified_name = qualify(scope_name, &name);
                let id = self.push_scoped_entity(
                    scope_id,
                    EntityKind::LocalVariable,
                    &name,
                    &qualified_name,
                    target,
                );
                if self.parsed.language == SourceLanguage::Rust {
                    if let Some(type_name) =
                        rust_local_type_name_from_assignment(assignment, &name, self.source)
                    {
                        self.register_rust_local_type(scope_id, &name, &type_name);
                    }
                }
                return Some(SymbolRef {
                    id,
                    exactness: Exactness::ParserVerified,
                    confidence: 1.0,
                });
            }
            let id = self.push_reference_entity(
                EntityKind::LocalVariable,
                &name,
                scope_name,
                target,
                "unresolved-write-target",
                0.55,
            );
            return Some(SymbolRef {
                id,
                exactness: Exactness::StaticHeuristic,
                confidence: 0.55,
            });
        }

        let id = self.push_expression_entity(
            &expression_label(target, self.source),
            scope_name,
            target,
            "assignment-target",
            0.75,
        );
        Some(SymbolRef {
            id,
            exactness: Exactness::StaticHeuristic,
            confidence: 0.75,
        })
    }

    fn expression_entity(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
    ) -> Option<SymbolRef> {
        if generic_assignment_target_is_identifier(node) {
            let name = node_text(node, self.source)?;
            return Some(
                self.resolve_or_reference_symbol(scope_id, &name, node)
                    .unwrap_or_else(|| SymbolRef {
                        id: self.push_reference_entity(
                            EntityKind::LocalVariable,
                            &name,
                            scope_name,
                            node,
                            "unresolved-read-reference",
                            0.55,
                        ),
                        exactness: Exactness::StaticHeuristic,
                        confidence: 0.55,
                    }),
            );
        }

        let id = self.push_expression_entity(
            &expression_label(node, self.source),
            scope_name,
            node,
            "expression",
            0.85,
        );
        Some(SymbolRef {
            id,
            exactness: Exactness::ParserVerified,
            confidence: 0.85,
        })
    }

    fn callee_entity(
        &mut self,
        call_node: Node<'_>,
        callee_node: Option<Node<'_>>,
        scope_id: &str,
        scope_name: &str,
    ) -> (String, Exactness, f64) {
        let Some(callee_node) = callee_node else {
            let id = self.push_reference_entity(
                EntityKind::Function,
                "unknown_callee",
                scope_name,
                self.parsed.tree.root_node(),
                "unresolved-callee",
                0.4,
            );
            return (id, Exactness::StaticHeuristic, 0.4);
        };
        if let Some(symbol) = self.resolve_rust_callee(call_node, callee_node, scope_id) {
            return (symbol.id, symbol.exactness, symbol.confidence);
        }
        let label = if self.parsed.language == SourceLanguage::Rust
            && call_node.kind() == "method_call_expression"
        {
            rust_method_call_label(call_node, callee_node, self.source)
                .unwrap_or_else(|| expression_label(callee_node, self.source))
        } else {
            expression_label(callee_node, self.source)
        };
        if self.parsed.language == SourceLanguage::Rust
            && call_node.kind() == "method_call_expression"
        {
            let id = self.push_reference_entity(
                EntityKind::Method,
                &label,
                scope_name,
                callee_node,
                "unresolved-callee",
                0.55,
            );
            return (id, Exactness::StaticHeuristic, 0.55);
        }
        if self.parsed.language == SourceLanguage::Rust && rust_method_label_parts(&label).is_some()
        {
            let id = self.push_reference_entity(
                EntityKind::Method,
                &label,
                scope_name,
                callee_node,
                "unresolved-callee",
                0.55,
            );
            return (id, Exactness::StaticHeuristic, 0.55);
        }
        if let Some(name) = generic_callee_symbol_name(&label) {
            if let Some(symbol) = self.resolve_symbol(scope_id, &name) {
                return (symbol.id, symbol.exactness, symbol.confidence);
            }
        }
        let kind = if label.contains('.') || label.contains("::") {
            EntityKind::Method
        } else {
            EntityKind::Function
        };
        let id = self.push_reference_entity(
            kind,
            &label,
            scope_name,
            callee_node,
            "unresolved-callee",
            0.55,
        );
        (id, Exactness::StaticHeuristic, 0.55)
    }

    fn resolve_rust_callee(
        &self,
        call_node: Node<'_>,
        callee_node: Node<'_>,
        scope_id: &str,
    ) -> Option<SymbolRef> {
        if self.parsed.language != SourceLanguage::Rust {
            return None;
        }
        if call_node.kind() == "method_call_expression" {
            let method = node_text(callee_node, self.source).map(clean_decl_name)?;
            if !looks_like_identifier(&method) {
                return None;
            }
            let receiver = call_node
                .child_by_field_name("receiver")
                .and_then(|node| node_text(node, self.source))
                .map(|text| compact_extracted_label(&text, callee_node))?;
            let receiver_type = self.resolve_rust_receiver_type(scope_id, &receiver)?;
            return self.resolve_rust_method_for_type(&receiver_type, &method);
        }
        let label = node_text(callee_node, self.source)
            .map(|text| compact_extracted_label(&text, callee_node))?;
        if let Some((receiver, method)) = rust_method_label_parts(&label) {
            let receiver_type = self.resolve_rust_receiver_type(scope_id, receiver)?;
            return self.resolve_rust_method_for_type(&receiver_type, method);
        }
        self.resolve_rust_path_symbol(scope_id, &label)
    }

    fn resolve_rust_receiver_type(&self, scope_id: &str, receiver: &str) -> Option<String> {
        if receiver == "self" {
            return self.rust_impl_type_for_scope(scope_id);
        }
        let mut current = Some(scope_id);
        while let Some(scope) = current {
            if let Some(type_name) = self
                .rust_local_types_by_scope
                .get(scope)
                .and_then(|types| types.get(receiver))
            {
                return Some(type_name.clone());
            }
            current = self.scope_parents.get(scope).map(String::as_str);
        }
        None
    }

    fn rust_impl_type_for_scope(&self, scope_id: &str) -> Option<String> {
        let mut current = Some(scope_id);
        while let Some(scope) = current {
            if let Some(type_name) = self.rust_impl_type_by_scope.get(scope) {
                return Some(type_name.clone());
            }
            current = self.scope_parents.get(scope).map(String::as_str);
        }
        None
    }

    fn resolve_rust_method_for_type(&self, type_name: &str, method: &str) -> Option<SymbolRef> {
        if self
            .rust_ambiguous_methods_by_type
            .get(type_name)
            .is_some_and(|methods| methods.contains(method))
        {
            return None;
        }
        self.rust_methods_by_type
            .get(type_name)
            .and_then(|methods| methods.get(method))
            .cloned()
    }

    fn resolve_rust_path_symbol(&self, scope_id: &str, raw_path: &str) -> Option<SymbolRef> {
        let parts = rust_path_parts(raw_path);
        if parts.is_empty() {
            return None;
        }
        if parts.len() == 1 {
            return self.resolve_symbol(scope_id, &parts[0]);
        }
        if parts[0] == "Self" {
            let type_name = self.rust_impl_type_for_scope(scope_id)?;
            return self.resolve_rust_method_for_type(&type_name, parts.last()?);
        }
        if looks_like_type_identifier(&parts[0]) {
            if let Some(method) = parts.last() {
                if let Some(symbol) = self.resolve_rust_method_for_type(&parts[0], method) {
                    return Some(symbol);
                }
            }
        }
        let mut scope = match parts[0].as_str() {
            "crate" => self.rust_root_module_scope(scope_id)?,
            "self" => self.rust_current_module_scope(scope_id)?,
            "super" => {
                let current = self.rust_current_module_scope(scope_id)?;
                self.rust_parent_module_scope(&current)?
            }
            first => self.resolve_symbol(scope_id, first)?.id,
        };
        for part in parts.iter().skip(1) {
            let symbol = self.resolve_symbol_in_scope(&scope, part)?;
            scope = symbol.id;
        }
        Some(SymbolRef {
            id: scope,
            exactness: Exactness::ParserVerified,
            confidence: 1.0,
        })
    }

    fn rust_root_module_scope(&self, scope_id: &str) -> Option<String> {
        let mut current = Some(scope_id);
        let mut last_module = None;
        while let Some(scope) = current {
            if self
                .entity_kinds
                .get(scope)
                .is_some_and(|kind| *kind == EntityKind::Module)
            {
                last_module = Some(scope.to_string());
            }
            current = self.scope_parents.get(scope).map(String::as_str);
        }
        last_module
    }

    fn rust_current_module_scope(&self, scope_id: &str) -> Option<String> {
        let mut current = Some(scope_id);
        while let Some(scope) = current {
            if self
                .entity_kinds
                .get(scope)
                .is_some_and(|kind| *kind == EntityKind::Module)
            {
                return Some(scope.to_string());
            }
            current = self.scope_parents.get(scope).map(String::as_str);
        }
        None
    }

    fn rust_parent_module_scope(&self, module_id: &str) -> Option<String> {
        let mut current = self.scope_parents.get(module_id).map(String::as_str);
        while let Some(scope) = current {
            if self
                .entity_kinds
                .get(scope)
                .is_some_and(|kind| *kind == EntityKind::Module)
            {
                return Some(scope.to_string());
            }
            current = self.scope_parents.get(scope).map(String::as_str);
        }
        None
    }

    fn resolve_symbol_in_scope(&self, scope_id: &str, name: &str) -> Option<SymbolRef> {
        if self
            .ambiguous_symbols_by_scope
            .get(scope_id)
            .is_some_and(|symbols| symbols.contains(name))
        {
            return None;
        }
        self.symbols_by_scope
            .get(scope_id)
            .and_then(|symbols| symbols.get(name))
            .cloned()
    }

    fn register_rust_import_binding(&mut self, scope_id: &str, binding: &ImportBinding) {
        if self.parsed.language != SourceLanguage::Rust {
            return;
        }
        let Some(path) = binding.imported_name.as_deref() else {
            return;
        };
        let Some(symbol) = self.resolve_rust_path_symbol(scope_id, path) else {
            return;
        };
        self.register_symbol_with(
            scope_id,
            &binding.local_name,
            &symbol.id,
            symbol.exactness,
            symbol.confidence,
        );
    }

    fn register_parser_observed_import_binding(
        &mut self,
        scope_id: &str,
        binding_id: &str,
        binding: &ImportBinding,
    ) {
        // Only Python relies on parser-observed import bindings to suppress
        // bogus unresolved-CALL escalation (its from-import aliases are not
        // otherwise resolvable in the local symbol table). JS/TS/JSX/TSX are
        // deliberately EXCLUDED: for those, a call to a VALID imported name is
        // already suppressed downstream by the whole-graph "candidates-exist"
        // definition lookup, so registering the binding here would only
        // suppress calls to a DANGLING import (one whose target the source
        // module does not export) — reopening the Q4 forward-hallucination
        // blind spot that validate-edit must catch.
        if self.parsed.language != SourceLanguage::Python {
            return;
        }
        if !looks_like_identifier(&binding.local_name) {
            return;
        }
        self.register_symbol_with(
            scope_id,
            &binding.local_name,
            binding_id,
            Exactness::StaticHeuristic,
            0.5,
        );
    }

    fn register_rust_parameter_type(&mut self, scope_id: &str, name: &str, node: Node<'_>) {
        if self.parsed.language != SourceLanguage::Rust {
            return;
        }
        let type_name = if name == "self" {
            self.rust_impl_type_for_scope(scope_id)
        } else {
            rust_parameter_type_name(node, self.source)
        };
        if let Some(type_name) = type_name {
            self.register_rust_local_type(scope_id, name, &type_name);
        }
    }

    fn register_rust_local_type(&mut self, scope_id: &str, name: &str, type_name: &str) {
        if !looks_like_identifier(name) || !looks_like_identifier(type_name) {
            return;
        }
        self.rust_local_types_by_scope
            .entry(scope_id.to_string())
            .or_default()
            .insert(name.to_string(), type_name.to_string());
    }

    fn register_rust_method_for_type(
        &mut self,
        type_name: &str,
        method: &str,
        id: &str,
        exactness: Exactness,
        confidence: f64,
    ) {
        let methods = self
            .rust_methods_by_type
            .entry(type_name.to_string())
            .or_default();
        if methods
            .get(method)
            .is_some_and(|existing| existing.id != id)
        {
            self.rust_ambiguous_methods_by_type
                .entry(type_name.to_string())
                .or_default()
                .insert(method.to_string());
            return;
        }
        methods.insert(
            method.to_string(),
            SymbolRef {
                id: id.to_string(),
                exactness,
                confidence,
            },
        );
    }

    fn resolve_or_reference_symbol(
        &mut self,
        scope_id: &str,
        name: &str,
        node: Node<'_>,
    ) -> Option<SymbolRef> {
        self.resolve_symbol(scope_id, name).or_else(|| {
            let id = self.push_reference_entity(
                EntityKind::LocalVariable,
                name,
                scope_id,
                node,
                "unresolved-read-reference",
                0.55,
            );
            Some(SymbolRef {
                id,
                exactness: Exactness::StaticHeuristic,
                confidence: 0.55,
            })
        })
    }

    fn resolve_symbol(&self, scope_id: &str, name: &str) -> Option<SymbolRef> {
        let mut current = Some(scope_id);
        while let Some(scope) = current {
            if self
                .ambiguous_symbols_by_scope
                .get(scope)
                .is_some_and(|symbols| symbols.contains(name))
            {
                return None;
            }
            if let Some(symbol) = self
                .symbols_by_scope
                .get(scope)
                .and_then(|symbols| symbols.get(name))
            {
                return Some(symbol.clone());
            }
            current = self.scope_parents.get(scope).map(String::as_str);
        }
        None
    }

    fn register_symbol_with(
        &mut self,
        scope_id: &str,
        name: &str,
        id: &str,
        exactness: Exactness,
        confidence: f64,
    ) {
        let symbols = self
            .symbols_by_scope
            .entry(scope_id.to_string())
            .or_default();
        if symbols.get(name).is_some_and(|existing| existing.id != id) {
            self.ambiguous_symbols_by_scope
                .entry(scope_id.to_string())
                .or_default()
                .insert(name.to_string());
            return;
        }
        symbols.insert(
            name.to_string(),
            SymbolRef {
                id: id.to_string(),
                exactness,
                confidence,
            },
        );
    }

    fn push_reference_entity(
        &mut self,
        kind: EntityKind,
        name: &str,
        scope_name: &str,
        node: Node<'_>,
        reason: &str,
        confidence: f64,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let qualified_name = if is_static_executable_reference(kind) {
            format!("static_reference:{name}")
        } else {
            qualify(scope_name, name)
        };
        let id = stable_entity_id(
            &self.parsed.repo_relative_path,
            format!("{}:{}(static-reference)", kind.id_prefix(), qualified_name),
        );
        self.entity_kinds.insert(id.clone(), kind);
        if !self.entity_indices.contains_key(&id) {
            let role = source_role_annotation(
                self.parsed.language,
                &self.parsed.repo_relative_path,
                kind,
                name,
                &qualified_name,
                Some(node),
                self.source,
            );
            let mut entity = Entity {
                id: id.clone(),
                kind,
                name: name.to_string(),
                qualified_name: qualified_name.clone(),
                repo_relative_path: self.parsed.repo_relative_path.clone(),
                source_span: Some(span),
                content_hash: None,
                file_hash: Some(self.file_hash.clone()),
                created_from: "tree-sitter-static-heuristic".to_string(),
                confidence,
                metadata: Default::default(),
            };
            entity
                .metadata
                .insert("source_role".to_string(), role.role.as_str().into());
            entity
                .metadata
                .insert("source_role_reason".to_string(), role.reason.into());
            entity
                .metadata
                .insert("source_role_source".to_string(), role.source.into());
            self.entity_source_roles.insert(id.clone(), role.role);
            self.entity_indices.insert(id.clone(), self.entities.len());
            self.entities.push(entity);
        }
        self.annotate_entity_source_role(&id, kind, name, &qualified_name, Some(node));
        if let Some(index) = self.entity_indices.get(&id).copied() {
            if let Some(entity) = self.entities.get_mut(index) {
                entity
                    .metadata
                    .insert("heuristic_reason".to_string(), reason.into());
                entity.metadata.insert(
                    "resolution".to_string(),
                    "unresolved_static_heuristic".into(),
                );
                entity.metadata.insert("phase".to_string(), "28".into());
            }
        }
        id
    }

    fn push_expression_entity(
        &mut self,
        _label: &str,
        scope_name: &str,
        node: Node<'_>,
        reason: &str,
        confidence: f64,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let compact_name = format!("expr@{}", node.start_byte());
        let id = self.push_entity(
            EntityKind::Expression,
            &compact_name,
            &qualify(scope_name, &compact_name),
            span,
        );
        if let Some(index) = self.entity_indices.get(&id).copied() {
            if let Some(entity) = self.entities.get_mut(index) {
                entity
                    .metadata
                    .insert("expression_reason".to_string(), reason.into());
                entity.confidence = entity.confidence.min(confidence);
            }
        }
        self.annotate_entity_source_role(
            &id,
            EntityKind::Expression,
            &compact_name,
            &qualify(scope_name, &compact_name),
            Some(node),
        );
        id
    }

    fn push_scoped_entity(
        &mut self,
        scope_id: &str,
        kind: EntityKind,
        name: &str,
        qualified_name: &str,
        node: Node<'_>,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity(kind, name, qualified_name, span.clone());
        self.annotate_entity_source_role(&id, kind, name, qualified_name, Some(node));
        let (exactness, confidence) = if node_has_error_or_missing_descendant(node) {
            if let Some(index) = self.entity_indices.get(&id).copied() {
                if let Some(entity) = self.entities.get_mut(index) {
                    annotate_untrusted_syntax_entity(
                        entity,
                        "declaration node contains tree-sitter ERROR or MISSING descendant",
                    );
                }
            }
            (Exactness::StaticHeuristic, 0.45)
        } else {
            (Exactness::ParserVerified, 1.0)
        };
        if is_scope_kind(kind) {
            self.scope_parents.insert(id.clone(), scope_id.to_string());
        }
        self.register_symbol_with(scope_id, name, &id, exactness, confidence);
        if self.parsed.language == SourceLanguage::Rust && kind == EntityKind::Method {
            if let Some(type_name) = rust_inherent_impl_type_name(node, self.source) {
                self.rust_impl_type_by_scope
                    .insert(id.clone(), type_name.clone());
                self.register_rust_method_for_type(&type_name, name, &id, exactness, confidence);
                if let Some(index) = self.entity_indices.get(&id).copied() {
                    if let Some(entity) = self.entities.get_mut(index) {
                        entity
                            .metadata
                            .insert("rust_impl_type".to_string(), type_name.into());
                        entity
                            .metadata
                            .insert("rust_method_resolution".to_string(), "inherent_impl".into());
                    }
                }
            }
        }
        self.push_edge_with(
            scope_id,
            RelationKind::Contains,
            &id,
            &span,
            exactness,
            confidence,
        );
        self.push_edge_with(
            &id,
            RelationKind::DefinedIn,
            scope_id,
            &span,
            exactness,
            confidence,
        );
        let relation = if matches!(
            kind,
            EntityKind::Class
                | EntityKind::Interface
                | EntityKind::Trait
                | EntityKind::Enum
                | EntityKind::Function
                | EntityKind::Method
                | EntityKind::Constructor
        ) {
            RelationKind::Defines
        } else {
            RelationKind::Declares
        };
        self.push_edge_with(scope_id, relation, &id, &span, exactness, confidence);
        id
    }

    fn push_entity(
        &mut self,
        kind: EntityKind,
        name: &str,
        qualified_name: &str,
        span: SourceSpan,
    ) -> String {
        let signature = format!(
            "{}@{}:{}-{}:{}",
            qualified_name,
            span.start_line,
            span.start_column.unwrap_or(1),
            span.end_line,
            span.end_column.unwrap_or(1)
        );
        let id = stable_entity_id_for_kind(
            &self.parsed.repo_relative_path,
            kind,
            qualified_name,
            Some(&signature),
        );
        self.entity_kinds.insert(id.clone(), kind);
        if !self.entity_indices.contains_key(&id) {
            let mut entity = Entity {
                id: id.clone(),
                kind,
                name: name.to_string(),
                qualified_name: qualified_name.to_string(),
                repo_relative_path: self.parsed.repo_relative_path.clone(),
                source_span: Some(span),
                content_hash: None,
                file_hash: Some(self.file_hash.clone()),
                created_from: "tree-sitter-language-frontend".to_string(),
                confidence: 1.0,
                metadata: Default::default(),
            };
            entity.metadata.insert("phase".to_string(), "27".into());
            entity.metadata.insert(
                "language_frontend".to_string(),
                self.parsed.language.as_str().into(),
            );
            let role = source_role_annotation(
                self.parsed.language,
                &self.parsed.repo_relative_path,
                kind,
                name,
                qualified_name,
                None,
                self.source,
            );
            entity
                .metadata
                .insert("source_role".to_string(), role.role.as_str().into());
            entity
                .metadata
                .insert("source_role_reason".to_string(), role.reason.into());
            entity
                .metadata
                .insert("source_role_source".to_string(), role.source.into());
            self.entity_source_roles.insert(id.clone(), role.role);
            self.entity_indices.insert(id.clone(), self.entities.len());
            self.entities.push(entity);
        }
        id
    }

    fn annotate_entity_source_role(
        &mut self,
        id: &str,
        kind: EntityKind,
        name: &str,
        qualified_name: &str,
        node: Option<Node<'_>>,
    ) {
        let role = source_role_annotation(
            self.parsed.language,
            &self.parsed.repo_relative_path,
            kind,
            name,
            qualified_name,
            node,
            self.source,
        );
        self.entity_source_roles.insert(id.to_string(), role.role);
        if let Some(index) = self.entity_indices.get(id).copied() {
            if let Some(entity) = self.entities.get_mut(index) {
                entity
                    .metadata
                    .insert("source_role".to_string(), role.role.as_str().into());
                entity
                    .metadata
                    .insert("source_role_reason".to_string(), role.reason.into());
                entity
                    .metadata
                    .insert("source_role_source".to_string(), role.source.into());
            }
        }
    }

    fn push_edge(
        &mut self,
        head_id: &str,
        relation: RelationKind,
        tail_id: &str,
        span: &SourceSpan,
    ) {
        self.push_edge_with(
            head_id,
            relation,
            tail_id,
            span,
            Exactness::ParserVerified,
            1.0,
        );
    }

    fn push_edge_with(
        &mut self,
        head_id: &str,
        relation: RelationKind,
        tail_id: &str,
        span: &SourceSpan,
        exactness: Exactness,
        confidence: f64,
    ) {
        let Some(head_kind) = self.entity_kinds.get(head_id).copied() else {
            return;
        };
        let Some(tail_kind) = self.entity_kinds.get(tail_id).copied() else {
            return;
        };
        if !relation_allows(relation, head_kind, tail_kind) {
            return;
        }
        let id = stable_edge_id(head_id, relation, tail_id, span);
        if !self.edge_ids.insert(id.clone()) {
            return;
        }
        let head_role = self
            .entity_source_roles
            .get(head_id)
            .copied()
            .unwrap_or(EvidenceRole::Unknown);
        let tail_role = self
            .entity_source_roles
            .get(tail_id)
            .copied()
            .unwrap_or(EvidenceRole::Unknown);
        let role = edge_role_annotation(relation, span, head_role, tail_role);
        let mut edge = Edge {
            id,
            head_id: head_id.to_string(),
            relation,
            tail_id: tail_id.to_string(),
            source_span: span.clone(),
            repo_commit: None,
            file_hash: Some(self.file_hash.clone()),
            extractor: "tree-sitter-language-frontend".to_string(),
            confidence,
            exactness,
            edge_class: if exactness == Exactness::StaticHeuristic {
                EdgeClass::BaseHeuristic
            } else {
                EdgeClass::BaseExact
            },
            context: edge_context_for_role(role.role),
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        };
        edge.metadata.insert(
            "phase".to_string(),
            if exactness == Exactness::StaticHeuristic {
                "28".into()
            } else {
                "27".into()
            },
        );
        edge.metadata.insert(
            "language_frontend".to_string(),
            self.parsed.language.as_str().into(),
        );
        edge.metadata
            .insert("source_role".to_string(), role.role.as_str().into());
        edge.metadata
            .insert("evidence_role".to_string(), role.role.as_str().into());
        edge.metadata
            .insert("classification_reason".to_string(), role.reason.into());
        edge.metadata
            .insert("classification_source".to_string(), role.source.into());
        edge.metadata
            .insert("head_source_role".to_string(), head_role.as_str().into());
        edge.metadata
            .insert("tail_source_role".to_string(), tail_role.as_str().into());
        if exactness == Exactness::StaticHeuristic {
            edge.metadata.insert("heuristic".to_string(), true.into());
            edge.metadata.insert(
                "resolution".to_string(),
                "unresolved_static_heuristic".into(),
            );
        }
        self.edges.push(edge);
    }
}

impl<'a> BasicEntityExtractor<'a> {
    fn new(parsed: &'a ParsedFile, source: &'a str) -> Self {
        Self {
            parsed,
            source,
            file_hash: content_hash(source),
            entities: Vec::new(),
            entity_indices: BTreeMap::new(),
            edges: Vec::new(),
            entity_kinds: BTreeMap::new(),
            entity_source_roles: BTreeMap::new(),
            edge_ids: BTreeSet::new(),
            scope_parents: BTreeMap::new(),
            symbols_by_scope: BTreeMap::new(),
            ambiguous_symbols_by_scope: BTreeMap::new(),
            table_symbols_by_scope: BTreeMap::new(),
            parameters_by_scope: BTreeMap::new(),
            return_sites_by_scope: BTreeMap::new(),
            test_file_id: None,
        }
    }

    fn extract(mut self) -> BasicExtraction {
        let file_record = FileRecord {
            repo_relative_path: self.parsed.repo_relative_path.clone(),
            file_hash: self.file_hash.clone(),
            language: Some(self.parsed.language.to_string()),
            size_bytes: self.parsed.byte_len as u64,
            indexed_at_unix_ms: None,
            metadata: parser_file_metadata(self.parsed),
        };

        let file_name = self.parsed.repo_relative_path.clone();
        let file_symbol_name = module_name_for_path(&self.parsed.repo_relative_path);
        let file_span = self.parsed.root_node.source_span.clone();
        let file_id = self.push_entity(
            EntityKind::File,
            &file_symbol_name,
            &file_name,
            file_span.clone(),
        );
        if is_test_file_path(&self.parsed.repo_relative_path) {
            let test_file_id = self.push_entity(
                EntityKind::TestFile,
                &file_name,
                &format!("test::{file_name}"),
                file_span.clone(),
            );
            self.push_edge(&file_id, RelationKind::Contains, &test_file_id, &file_span);
            self.push_edge(&test_file_id, RelationKind::DefinedIn, &file_id, &file_span);
            self.test_file_id = Some(test_file_id);
        }

        let module_name = module_name_for_path(&self.parsed.repo_relative_path);
        let module_id = self.push_entity(
            EntityKind::Module,
            &module_name,
            &module_name,
            self.parsed.root_node.source_span.clone(),
        );
        self.scope_parents
            .insert(module_id.clone(), file_id.clone());
        let module_span = self.parsed.root_node.source_span.clone();
        self.push_edge(&file_id, RelationKind::Contains, &module_id, &module_span);
        self.push_edge(&file_id, RelationKind::Defines, &module_id, &module_span);
        self.push_edge(&module_id, RelationKind::DefinedIn, &file_id, &module_span);

        self.visit_children(self.parsed.tree.root_node(), &module_id, &module_name);
        self.recover_syntax_error_declarations(&module_id, &module_name);

        BasicExtraction {
            file: file_record,
            entities: self.entities,
            edges: self.edges,
        }
    }

    fn visit_children(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, scope_id, scope_name);
        }
    }

    fn recover_syntax_error_declarations(&mut self, scope_id: &str, scope_name: &str) {
        if !self.parsed.has_syntax_errors() {
            return;
        }
        for declaration in recoverable_declarations_from_source(
            self.parsed.language,
            &self.parsed.repo_relative_path,
            self.source,
        ) {
            if self.entities.iter().any(|entity| {
                entity.kind == declaration.kind
                    && entity.name == declaration.name
                    && entity.repo_relative_path == self.parsed.repo_relative_path
            }) {
                continue;
            }
            let qualified_name = qualify(scope_name, &declaration.name);
            let id = self.push_entity_with(
                declaration.kind,
                &declaration.name,
                &qualified_name,
                declaration.span.clone(),
                "tree-sitter-syntax-recovery",
                0.45,
            );
            if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
                annotate_untrusted_syntax_entity(
                    entity,
                    "recovered declaration from syntax-error source prefix",
                );
                entity
                    .metadata
                    .insert("syntax_recovery".to_string(), true.into());
            }
            if is_scope_kind(declaration.kind) {
                self.scope_parents.insert(id.clone(), scope_id.to_string());
            }
            self.register_symbol_with(
                scope_id,
                &declaration.name,
                &id,
                Exactness::StaticHeuristic,
                0.45,
            );
            self.push_edge_with(
                scope_id,
                RelationKind::Contains,
                &id,
                &declaration.span,
                Exactness::StaticHeuristic,
                0.45,
            );
            self.push_edge_with(
                &id,
                RelationKind::DefinedIn,
                scope_id,
                &declaration.span,
                Exactness::StaticHeuristic,
                0.45,
            );
            let relation = if matches!(
                declaration.kind,
                EntityKind::Class
                    | EntityKind::Interface
                    | EntityKind::Trait
                    | EntityKind::Enum
                    | EntityKind::Function
                    | EntityKind::Method
                    | EntityKind::Constructor
            ) {
                RelationKind::Defines
            } else {
                RelationKind::Declares
            };
            self.push_edge_with(
                scope_id,
                relation,
                &id,
                &declaration.span,
                Exactness::StaticHeuristic,
                0.45,
            );
        }
    }

    fn visit_node(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        if node.is_error() || node.is_missing() {
            return;
        }
        let node_untrusted = node_has_error_or_missing_descendant(node);

        match node.kind() {
            "class_declaration" => {
                if let Some(name) = name_from_field_or_child(node, self.source) {
                    let qualified_name = qualify(scope_name, &name);
                    let id = self.push_scoped_entity(
                        scope_id,
                        EntityKind::Class,
                        &name,
                        &qualified_name,
                        node,
                    );
                    if node_untrusted {
                        return;
                    }
                    self.visit_children(node, &id, &qualified_name);
                    return;
                }
            }
            "interface_declaration" => {
                if let Some(name) = name_from_field_or_child(node, self.source) {
                    let qualified_name = qualify(scope_name, &name);
                    let id = self.push_scoped_entity(
                        scope_id,
                        EntityKind::Interface,
                        &name,
                        &qualified_name,
                        node,
                    );
                    if node_untrusted {
                        return;
                    }
                    self.visit_children(node, &id, &qualified_name);
                    return;
                }
            }
            "function_declaration" | "generator_function_declaration" => {
                if let Some(name) = name_from_field_or_child(node, self.source) {
                    let qualified_name = qualify(scope_name, &name);
                    let id = self.push_scoped_entity(
                        scope_id,
                        EntityKind::Function,
                        &name,
                        &qualified_name,
                        node,
                    );
                    if node_untrusted {
                        return;
                    }
                    self.extract_parameters(node, &id, &qualified_name);
                    self.visit_children(node, &id, &qualified_name);
                    return;
                }
            }
            "method_definition" | "method_signature" => {
                let Some(name) = name_from_field_or_child(node, self.source) else {
                    self.visit_children(node, scope_id, scope_name);
                    return;
                };
                let kind = if name == "constructor" {
                    EntityKind::Constructor
                } else {
                    EntityKind::Method
                };
                let qualified_name = qualify(scope_name, &name);
                let id = self.push_scoped_entity(scope_id, kind, &name, &qualified_name, node);
                if node_untrusted {
                    return;
                }
                self.extract_parameters(node, &id, &qualified_name);
                self.visit_children(node, &id, &qualified_name);
                return;
            }
            "variable_declarator" => {
                if let Some(name) = variable_name(node, self.source) {
                    let qualified_name = qualify(scope_name, &name);
                    let target_id = self.push_scoped_entity(
                        scope_id,
                        EntityKind::LocalVariable,
                        &name,
                        &qualified_name,
                        node,
                    );
                    if node_untrusted {
                        return;
                    }
                    let span = source_span_for_node(&self.parsed.repo_relative_path, node);
                    self.push_edge(scope_id, RelationKind::Writes, &target_id, &span);
                    if looks_like_table_constant_name(&name) {
                        self.push_table_constant_entity(scope_id, &name, &qualified_name, node);
                    }
                    if let Some(value) = variable_declarator_value_node(node) {
                        let value_span =
                            source_span_for_node(&self.parsed.repo_relative_path, value);
                        if let Some(source_id) = self.expression_entity(value, scope_id, scope_name)
                        {
                            self.push_edge(
                                &target_id,
                                RelationKind::AssignedFrom,
                                &source_id,
                                &value_span,
                            );
                            self.push_edge(
                                &source_id,
                                RelationKind::FlowsTo,
                                &target_id,
                                &value_span,
                            );
                        }
                        self.extract_return_value_assignment_flow(
                            value,
                            &SymbolRef {
                                id: target_id.clone(),
                                exactness: Exactness::ParserVerified,
                                confidence: 1.0,
                            },
                            &value_span,
                            scope_id,
                            scope_name,
                        );
                        self.extract_reads_from_expression(value, scope_id, Some(&target_id));
                    }
                }
            }
            "assignment_expression" | "augmented_assignment_expression" if !node_untrusted => {
                self.extract_assignment(node, scope_id, scope_name);
            }
            "call_expression" if !node_untrusted => {
                self.extract_call(node, scope_id, scope_name);
            }
            "new_expression" if !node_untrusted => {
                self.extract_new_expression(node, scope_id, scope_name);
            }
            "await_expression" if !node_untrusted => {
                self.extract_await_expression(node, scope_id, scope_name);
            }
            "return_statement" if !node_untrusted => {
                self.extract_return(node, scope_id, scope_name);
            }
            "import_statement" | "import_from_statement" => {
                if node_untrusted {
                    return;
                }
                self.extract_import_statement(scope_id, scope_name, node);
            }
            "export_statement" => {
                if node_untrusted {
                    return;
                }
                let id = self.extract_export_statement(scope_id, scope_name, node);
                if let Some((default_kind, declaration)) =
                    default_export_declaration(node, self.source)
                {
                    let default_span =
                        source_span_for_node(&self.parsed.repo_relative_path, declaration);
                    let default_name = "default";
                    let default_qualified_name = qualify(scope_name, default_name);
                    let default_id = self.push_scoped_entity(
                        scope_id,
                        default_kind,
                        default_name,
                        &default_qualified_name,
                        declaration,
                    );
                    if let Some(entity) = self
                        .entities
                        .iter_mut()
                        .find(|entity| entity.id == default_id)
                    {
                        entity
                            .metadata
                            .insert("export_kind".to_string(), "default".into());
                    }
                    self.push_edge(scope_id, RelationKind::Exports, &default_id, &default_span);
                }
                if let Some(export) = self.entities.iter_mut().find(|entity| entity.id == id) {
                    export
                        .metadata
                        .insert("tier1_export_observed".to_string(), true.into());
                }
            }
            _ => {}
        }

        self.visit_children(node, scope_id, scope_name);
    }

    fn extract_import_statement(&mut self, scope_id: &str, scope_name: &str, node: Node<'_>) {
        let name = statement_label(node, self.source);
        let qualified_name = qualify(scope_name, &name);
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity(EntityKind::Import, &name, &qualified_name, span.clone());
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(scope_id, RelationKind::Imports, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
        let statement_binding = statement_import_binding(self.parsed.language, &name);
        annotate_import_artifact(&mut self.entities, &mut self.edges, &id, &statement_binding);

        for (index, binding) in import_bindings_for_node(self.parsed.language, node, self.source)
            .into_iter()
            .enumerate()
        {
            let binding_id = self.push_import_binding(scope_id, scope_name, node, index, &binding);
            annotate_import_artifact(&mut self.entities, &mut self.edges, &binding_id, &binding);
            self.register_parser_observed_import_binding(scope_id, &binding_id, &binding);
            self.push_unresolved_import_target_reference(scope_id, scope_name, node, &binding);
        }
    }

    /// See the Tier-1 extractor's equivalent: the import TARGET becomes a
    /// reference entity + StaticHeuristic IMPORTS edge so nonexistent import
    /// targets are visible unresolved-reference lane facts.
    fn push_unresolved_import_target_reference(
        &mut self,
        scope_id: &str,
        scope_name: &str,
        node: Node<'_>,
        binding: &ImportBinding,
    ) {
        let Some(target_label) = unresolved_import_target_label(self.parsed.language, binding)
        else {
            return;
        };
        let target_id = self.push_reference_entity(
            EntityKind::Import,
            &target_label,
            scope_name,
            node,
            "unresolved-import-target",
            0.5,
        );
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        self.push_edge_with(
            scope_id,
            RelationKind::Imports,
            &target_id,
            &span,
            Exactness::StaticHeuristic,
            0.5,
        );
    }

    fn push_import_binding(
        &mut self,
        scope_id: &str,
        scope_name: &str,
        node: Node<'_>,
        index: usize,
        binding: &ImportBinding,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let qualified_name = qualify(
            scope_name,
            &format!(
                "import:{}:{}#{}",
                binding.import_kind, binding.local_name, index
            ),
        );
        let id = self.push_entity(
            EntityKind::Import,
            &binding.local_name,
            &qualified_name,
            span.clone(),
        );
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(scope_id, RelationKind::Imports, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
        id
    }

    fn extract_export_statement(
        &mut self,
        scope_id: &str,
        scope_name: &str,
        node: Node<'_>,
    ) -> String {
        let name = statement_label(node, self.source);
        let qualified_name = qualify(scope_name, &name);
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity(EntityKind::Export, &name, &qualified_name, span.clone());
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(scope_id, RelationKind::Exports, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
        if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
            annotate_export_metadata(entity, self.parsed.language, &name);
        }
        for (index, binding) in reexport_bindings_for_node(self.parsed.language, node, self.source)
            .into_iter()
            .enumerate()
        {
            let binding_id = self.push_import_binding(scope_id, scope_name, node, index, &binding);
            annotate_import_artifact(&mut self.entities, &mut self.edges, &binding_id, &binding);
        }
        for (index, (local_name, exported_name)) in
            local_export_bindings_for_node(self.parsed.language, node, self.source)
                .into_iter()
                .enumerate()
        {
            let export_span = source_span_for_node(&self.parsed.repo_relative_path, node);
            let export_id = self.push_entity(
                EntityKind::Export,
                &exported_name,
                &qualify(
                    scope_name,
                    &format!("export:{exported_name}@{}", node.start_byte() + index),
                ),
                export_span.clone(),
            );
            self.push_edge(scope_id, RelationKind::Contains, &export_id, &export_span);
            self.push_edge(scope_id, RelationKind::Exports, &export_id, &export_span);
            self.push_edge(&export_id, RelationKind::DefinedIn, scope_id, &export_span);
            if let Some(entity) = self
                .entities
                .iter_mut()
                .find(|entity| entity.id == export_id)
            {
                annotate_local_export_metadata(entity, &local_name);
            }
        }
        id
    }

    fn extract_parameters(&mut self, node: Node<'_>, parent_id: &str, parent_name: &str) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "formal_parameters" {
                self.extract_parameters_from_list(child, parent_id, parent_name);
            }
        }
    }

    fn extract_parameters_from_list(
        &mut self,
        parameters: Node<'_>,
        parent_id: &str,
        parent_name: &str,
    ) {
        let mut cursor = parameters.walk();
        for parameter in parameters.named_children(&mut cursor) {
            if let Some(name) = parameter_name(parameter, self.source) {
                let qualified_name = qualify(parent_name, &name);
                let id = self.push_scoped_entity(
                    parent_id,
                    EntityKind::Parameter,
                    &name,
                    &qualified_name,
                    parameter,
                );
                self.parameters_by_scope
                    .entry(parent_id.to_string())
                    .or_default()
                    .push(id);
            }
        }
    }

    fn extract_assignment(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let Some(left) = node.child_by_field_name("left") else {
            return;
        };
        let Some(right) = node.child_by_field_name("right") else {
            return;
        };
        let Some(target) = self.assignment_target_entity(left, scope_id, scope_name) else {
            return;
        };
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        self.push_edge_with(
            scope_id,
            RelationKind::Writes,
            &target.id,
            &span,
            target.exactness,
            target.confidence,
        );
        if left.kind() == "member_expression" || left.kind() == "subscript_expression" {
            self.push_edge_with(
                scope_id,
                RelationKind::Mutates,
                &target.id,
                &span,
                Exactness::StaticHeuristic,
                target.confidence.min(0.75),
            );
        }

        if let Some(source_id) = self.expression_entity(right, scope_id, scope_name) {
            self.push_edge_with(
                &target.id,
                RelationKind::AssignedFrom,
                &source_id,
                &span,
                target.exactness,
                target.confidence,
            );
            self.push_edge_with(
                &source_id,
                RelationKind::FlowsTo,
                &target.id,
                &span,
                target.exactness,
                target.confidence,
            );
        }
        let right_span = source_span_for_node(&self.parsed.repo_relative_path, right);
        self.extract_return_value_assignment_flow(
            right,
            &target,
            &right_span,
            scope_id,
            scope_name,
        );
        self.extract_reads_from_expression(right, scope_id, Some(&target.id));
    }

    fn extract_call(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let callee_node = node.child_by_field_name("function");
        let callee_label = callee_node
            .and_then(|callee| {
                node_text(callee, self.source).map(|text| compact_extracted_label(&text, callee))
            })
            .unwrap_or_else(|| "unknown_callee".to_string());
        let callsite_name = format!("call:{callee_label}");
        let callsite_qualified = qualify(
            scope_name,
            &format!("{callsite_name}@{}", node.start_byte()),
        );
        let callsite_id = self.push_entity(
            EntityKind::CallSite,
            &callsite_name,
            &callsite_qualified,
            span.clone(),
        );
        self.scope_parents
            .insert(callsite_id.clone(), scope_id.to_string());
        self.push_edge(scope_id, RelationKind::Contains, &callsite_id, &span);
        self.push_edge(&callsite_id, RelationKind::DefinedIn, scope_id, &span);

        let (callee_id, exactness, confidence) =
            self.callee_entity(callee_node, scope_id, scope_name);
        self.push_edge_with(
            scope_id,
            RelationKind::Calls,
            &callee_id,
            &span,
            exactness,
            confidence,
        );
        self.push_edge_with(
            &callsite_id,
            RelationKind::Callee,
            &callee_id,
            &span,
            exactness,
            confidence,
        );
        self.extract_obvious_mutation_call(scope_id, &callee_label, &span);

        if let Some(arguments) = node.child_by_field_name("arguments") {
            self.extract_call_arguments(arguments, &callsite_id, scope_id, scope_name);
            self.extract_argument_to_parameter_flows(arguments, scope_id, &callee_id);
        }
        self.extract_extended_call_relations(
            node,
            &callsite_id,
            scope_id,
            scope_name,
            &callee_label,
        );
    }

    fn extract_call_arguments(
        &mut self,
        arguments: Node<'_>,
        callsite_id: &str,
        scope_id: &str,
        scope_name: &str,
    ) {
        let mut index = 0usize;
        let mut cursor = arguments.walk();
        for argument in arguments.named_children(&mut cursor) {
            if argument.kind() == "comment" {
                continue;
            }
            let span = source_span_for_node(&self.parsed.repo_relative_path, argument);
            let arg_name = format!("argument_{index}@{}", argument.start_byte());
            let arg_id = self.push_entity(
                EntityKind::Expression,
                &arg_name,
                &qualify(scope_name, &format!("{arg_name}@{}", argument.start_byte())),
                span.clone(),
            );
            let relation = match index {
                0 => RelationKind::Argument0,
                1 => RelationKind::Argument1,
                _ => RelationKind::ArgumentN,
            };
            self.push_edge(callsite_id, relation, &arg_id, &span);
            if let Some(source_id) = self.expression_entity(argument, scope_id, scope_name) {
                self.push_edge(&source_id, RelationKind::FlowsTo, &arg_id, &span);
            }
            self.extract_reads_from_expression(argument, scope_id, None);
            index += 1;
        }
    }

    fn extract_argument_to_parameter_flows(
        &mut self,
        arguments: Node<'_>,
        scope_id: &str,
        callee_id: &str,
    ) {
        let Some(parameters) = self.parameters_by_scope.get(callee_id).cloned() else {
            return;
        };
        let mut index = 0usize;
        let mut cursor = arguments.walk();
        for argument in arguments.named_children(&mut cursor) {
            if argument.kind() == "comment" {
                continue;
            }
            let Some(parameter_id) = parameters.get(index) else {
                index += 1;
                continue;
            };
            let Some(source) = self.direct_argument_symbol(argument, scope_id) else {
                index += 1;
                continue;
            };
            let span = source_span_for_node(&self.parsed.repo_relative_path, argument);
            self.push_edge_with(
                &source.id,
                RelationKind::FlowsTo,
                parameter_id,
                &span,
                source.exactness,
                source.confidence,
            );
            index += 1;
        }
    }

    fn direct_argument_symbol(&self, argument: Node<'_>, scope_id: &str) -> Option<SymbolRef> {
        if argument.kind() != "identifier" {
            return None;
        }
        let name = node_text(argument, self.source)?;
        self.resolve_symbol(scope_id, &name)
    }

    fn extract_obvious_mutation_call(
        &mut self,
        scope_id: &str,
        callee_label: &str,
        span: &SourceSpan,
    ) {
        let Some((receiver, _method)) = simple_mutation_method_receiver(callee_label) else {
            return;
        };
        let Some(symbol) = self.resolve_symbol(scope_id, receiver) else {
            return;
        };
        self.push_edge_with(
            scope_id,
            RelationKind::Mutates,
            &symbol.id,
            span,
            Exactness::StaticHeuristic,
            symbol.confidence.min(0.72),
        );
    }

    fn extract_extended_call_relations(
        &mut self,
        node: Node<'_>,
        callsite_id: &str,
        scope_id: &str,
        scope_name: &str,
        callee_label: &str,
    ) {
        let arguments = call_argument_nodes(node);
        self.extract_dynamic_module_call(node, scope_id, scope_name, callee_label, &arguments);
        self.extract_route_call(
            node,
            callsite_id,
            scope_id,
            scope_name,
            callee_label,
            &arguments,
        );
        self.extract_security_call(
            node,
            callsite_id,
            scope_id,
            scope_name,
            callee_label,
            &arguments,
        );
        self.extract_event_call(
            node,
            callsite_id,
            scope_id,
            scope_name,
            callee_label,
            &arguments,
        );
        self.extract_persistence_call(node, scope_id, scope_name, callee_label, &arguments);
        self.extract_test_call(node, scope_id, scope_name, callee_label, &arguments);
    }

    fn extract_dynamic_module_call(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
        callee_label: &str,
        arguments: &[Node<'_>],
    ) {
        let lower = callee_label.to_ascii_lowercase();
        let import_kind = if lower == "import" {
            "dynamic_import"
        } else if lower == "require" || lower.ends_with(".require") {
            "dynamic_require"
        } else {
            return;
        };
        let Some(first_argument) = arguments.first().copied() else {
            return;
        };

        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let literal_target = string_literal_value(first_argument, self.source);
        let (name, binding) = if let Some(target) = literal_target {
            (
                target.clone(),
                ImportBinding::target_unsupported(
                    target.clone(),
                    None,
                    Some(target),
                    format!("{import_kind}_literal"),
                    "dynamic module target resolution requires a module resolver",
                ),
            )
        } else {
            let name = format!("{import_kind}@{}", node.start_byte());
            (
                name.clone(),
                ImportBinding::target_unsupported(
                    name,
                    None,
                    None,
                    format!("{import_kind}_computed"),
                    "computed dynamic module target is unsupported for exact import proof",
                ),
            )
        };
        let id = self.push_entity_with(
            EntityKind::Import,
            &name,
            &qualify(scope_name, &format!("{import_kind}:{name}")),
            span.clone(),
            "tree-sitter-dynamic-module-heuristic",
            0.52,
        );
        self.push_edge_with(
            scope_id,
            RelationKind::Contains,
            &id,
            &span,
            Exactness::StaticHeuristic,
            0.52,
        );
        self.push_edge_with(
            scope_id,
            RelationKind::Imports,
            &id,
            &span,
            Exactness::StaticHeuristic,
            0.52,
        );
        self.push_edge_with(
            &id,
            RelationKind::DefinedIn,
            scope_id,
            &span,
            Exactness::StaticHeuristic,
            0.52,
        );
        annotate_import_artifact(&mut self.entities, &mut self.edges, &id, &binding);
        if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
            entity.metadata.insert("tier".to_string(), "5".into());
            entity
                .metadata
                .insert("dynamic_module_call".to_string(), true.into());
        }
        for edge in self
            .edges
            .iter_mut()
            .filter(|edge| edge.tail_id == id || edge.head_id == id)
        {
            edge.metadata.insert("tier".to_string(), "5".into());
            edge.metadata
                .insert("dynamic_module_call".to_string(), true.into());
        }
    }

    fn extract_route_call(
        &mut self,
        node: Node<'_>,
        callsite_id: &str,
        scope_id: &str,
        scope_name: &str,
        callee_label: &str,
        arguments: &[Node<'_>],
    ) {
        let Some(method) = route_method(callee_label) else {
            return;
        };
        let Some(path) = first_string_argument(arguments, self.source) else {
            return;
        };
        if !path.starts_with('/') {
            return;
        }

        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let route_name = format!("{} {}", method.to_ascii_uppercase(), path);
        let route_id = self.push_extended_entity(
            EntityKind::Route,
            &route_name,
            scope_name,
            node,
            HeuristicTag::new("express-route", "express", 0.72),
        );
        let endpoint_id = self.push_extended_entity(
            EntityKind::Endpoint,
            &route_name,
            scope_name,
            node,
            HeuristicTag::new("express-route", "express", 0.72),
        );
        self.push_edge(&route_id, RelationKind::Exposes, &endpoint_id, &span);
        self.push_extended_edge(
            &route_id,
            RelationKind::Exposes,
            &endpoint_id,
            &span,
            HeuristicTag::new("express-route", "express", 0.72),
        );

        for argument in arguments.iter().skip(1) {
            if let Some(handler) = self.resolved_direct_executable_symbol(*argument, scope_id) {
                self.push_edge_with(
                    &route_id,
                    RelationKind::Handles,
                    &handler.id,
                    &span,
                    handler.exactness,
                    handler.confidence,
                );
                continue;
            }
            if let Some(handler_id) = self.handler_entity(*argument, scope_id, scope_name) {
                self.push_extended_edge(
                    &route_id,
                    RelationKind::Handles,
                    &handler_id,
                    &span,
                    HeuristicTag::new("express-route-handler", "express", 0.62),
                );
            }
        }

        self.push_extended_edge(
            callsite_id,
            RelationKind::TrustBoundary,
            &route_id,
            &span,
            HeuristicTag::new("express-route-boundary", "express", 0.55),
        );
    }

    fn extract_security_call(
        &mut self,
        node: Node<'_>,
        callsite_id: &str,
        scope_id: &str,
        scope_name: &str,
        callee_label: &str,
        arguments: &[Node<'_>],
    ) {
        let lower = callee_label.to_ascii_lowercase();
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);

        if contains_any(
            &lower,
            &["authorize", "authz", "requireauth", "authenticate"],
        ) {
            let policy = first_string_argument(arguments, self.source)
                .unwrap_or_else(|| callee_label.to_string());
            let policy_id = self.push_extended_entity(
                EntityKind::AuthPolicy,
                &policy,
                scope_name,
                node,
                HeuristicTag::new("auth-policy-call", "auth", 0.64),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::Authorizes,
                &policy_id,
                &span,
                HeuristicTag::new("auth-policy-call", "auth", 0.64),
            );
        }

        if contains_any(&lower, &["role", "hasrole", "requirerole", "checkrole"]) {
            let literal_role = first_string_argument(arguments, self.source);
            let role = literal_role.clone().unwrap_or_else(|| "role".to_string());
            let exact_role_check = literal_role.is_some() && is_direct_role_check_label(&lower);
            let role_id = if exact_role_check {
                self.push_entity(
                    EntityKind::Role,
                    &role,
                    &qualify(scope_name, &role),
                    span.clone(),
                )
            } else {
                self.push_extended_entity(
                    EntityKind::Role,
                    &role,
                    scope_name,
                    node,
                    HeuristicTag::new("role-check-call", "auth", 0.66),
                )
            };
            if exact_role_check {
                self.push_edge(callsite_id, RelationKind::ChecksRole, &role_id, &span);
            } else {
                self.push_extended_edge(
                    callsite_id,
                    RelationKind::ChecksRole,
                    &role_id,
                    &span,
                    HeuristicTag::new("role-check-call", "auth", 0.66),
                );
            }
        }

        if contains_any(
            &lower,
            &["permission", "can(", ".can", "allowedto", "permit"],
        ) {
            let permission = first_string_argument(arguments, self.source)
                .unwrap_or_else(|| "permission".to_string());
            let permission_id = self.push_extended_entity(
                EntityKind::Permission,
                &permission,
                scope_name,
                node,
                HeuristicTag::new("permission-check-call", "auth", 0.64),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::ChecksPermission,
                &permission_id,
                &span,
                HeuristicTag::new("permission-check-call", "auth", 0.64),
            );
        }

        if contains_any(
            &lower,
            &[
                "sanitize",
                "escape",
                "dompurify.sanitize",
                "xss",
                "cleanhtml",
            ],
        ) {
            if let Some(target) = arguments.first().copied() {
                if let Some(target_id) = self.expression_entity(target, scope_id, scope_name) {
                    self.push_extended_edge(
                        callsite_id,
                        RelationKind::Sanitizes,
                        &target_id,
                        &span,
                        HeuristicTag::new("sanitizer-call", "validation", 0.67),
                    );
                }
            }
        }

        if contains_any(
            &lower,
            &["validate", "safeparse", ".parse", "isemail", "schema"],
        ) && !contains_any(&lower, &["json.parse"])
        {
            if let Some(target) = arguments.first().copied() {
                if let Some(target_id) = self.expression_entity(target, scope_id, scope_name) {
                    self.push_extended_edge(
                        callsite_id,
                        RelationKind::Validates,
                        &target_id,
                        &span,
                        HeuristicTag::new("validator-call", "validation", 0.63),
                    );
                }
            }
        }

        if contains_any(
            &lower,
            &[
                "cors",
                "helmet",
                "csrf",
                "ratelimit",
                "cookieparser",
                "bodyparser",
                "withauth",
            ],
        ) {
            let middleware_id = self.push_extended_entity(
                EntityKind::Middleware,
                callee_label,
                scope_name,
                node,
                HeuristicTag::new("middleware-boundary-call", "http", 0.62),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::TrustBoundary,
                &middleware_id,
                &span,
                HeuristicTag::new("middleware-boundary-call", "http", 0.62),
            );
        }

        for argument in arguments {
            if let Some(argument_text) = node_text(*argument, self.source) {
                if contains_taint_source(&argument_text) {
                    let target_id = self.push_expression_entity(
                        &expression_label(*argument, self.source),
                        scope_name,
                        *argument,
                        "taint-target",
                        0.58,
                    );
                    let source_name =
                        synthetic_source_identity("taint_source", *argument, &argument_text);
                    let source_id = self.push_extended_entity(
                        EntityKind::Expression,
                        &source_name,
                        scope_name,
                        *argument,
                        HeuristicTag::new("taint-source-pattern", "security", 0.58),
                    );
                    if let Some(entity) = self
                        .entities
                        .iter_mut()
                        .find(|entity| entity.id == source_id)
                    {
                        entity.metadata.insert(
                            "identity_material".to_string(),
                            "source_span_and_text_hash".into(),
                        );
                        entity.metadata.insert(
                            "source_text_hash".to_string(),
                            content_hash(&argument_text).into(),
                        );
                        entity.metadata.insert(
                            "debug_display".to_string(),
                            "load_source_snippet_from_span".into(),
                        );
                    }
                    self.push_extended_edge(
                        &source_id,
                        RelationKind::SourceOfTaint,
                        &target_id,
                        &span,
                        HeuristicTag::new("taint-source-pattern", "security", 0.58),
                    );
                }
            }
        }

        if contains_any(
            &lower,
            &[
                "res.send",
                "res.json",
                "reply.send",
                "redirect",
                "render",
                "eval",
                "exec",
                "innerhtml",
                ".query",
            ],
        ) {
            let target = arguments.first().copied().unwrap_or(node);
            let sink_id = self.push_expression_entity(
                &format!("sink:{}", expression_label(target, self.source)),
                scope_name,
                target,
                "sink-target",
                0.58,
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::SinksTo,
                &sink_id,
                &span,
                HeuristicTag::new("sink-call", "security", 0.58),
            );
        }
    }

    fn extract_event_call(
        &mut self,
        node: Node<'_>,
        callsite_id: &str,
        scope_id: &str,
        scope_name: &str,
        callee_label: &str,
        arguments: &[Node<'_>],
    ) {
        let lower = callee_label.to_ascii_lowercase();
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let literal_channel = first_string_argument(arguments, self.source);
        let channel = literal_channel
            .clone()
            .unwrap_or_else(|| callee_label.to_string());

        if label_ends_with(&lower, "emit") {
            let event_id = self.push_extended_entity(
                EntityKind::Event,
                &channel,
                scope_name,
                node,
                HeuristicTag::new("event-emit-call", "event-emitter", 0.66),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::Emits,
                &event_id,
                &span,
                HeuristicTag::new("event-emit-call", "event-emitter", 0.66),
            );
        }

        if contains_any(&lower, &["publish", "producer.send", "channel.send"]) {
            let topic_id = self.push_extended_entity(
                EntityKind::Topic,
                &channel,
                scope_name,
                node,
                HeuristicTag::new("message-publish-call", "messaging", 0.64),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::Publishes,
                &topic_id,
                &span,
                HeuristicTag::new("message-publish-call", "messaging", 0.64),
            );
        }

        if label_ends_with(&lower, "on") || contains_any(&lower, &["addeventlistener", "listen"]) {
            let event_id = self.push_extended_entity(
                EntityKind::Event,
                &channel,
                scope_name,
                node,
                HeuristicTag::new("event-listener-call", "event-emitter", 0.64),
            );
            if literal_channel.is_some() {
                self.push_edge(callsite_id, RelationKind::ListensTo, &event_id, &span);
            }
            self.push_extended_edge(
                callsite_id,
                RelationKind::ListensTo,
                &event_id,
                &span,
                HeuristicTag::new("event-listener-call", "event-emitter", 0.64),
            );
            for argument in arguments.iter().skip(1) {
                if let Some(handler) = self.resolved_direct_executable_symbol(*argument, scope_id) {
                    self.push_edge_with(
                        &event_id,
                        RelationKind::Handles,
                        &handler.id,
                        &span,
                        handler.exactness,
                        handler.confidence,
                    );
                }
            }
            self.push_handler_edge(
                arguments,
                callsite_id,
                scope_id,
                scope_name,
                &span,
                HeuristicTag::new("event-listener-handler", "event-emitter", 0.58),
            );
        }

        if contains_any(&lower, &["consume", "process"]) {
            let topic_id = self.push_extended_entity(
                EntityKind::Topic,
                &channel,
                scope_name,
                node,
                HeuristicTag::new("message-consume-call", "messaging", 0.64),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::Consumes,
                &topic_id,
                &span,
                HeuristicTag::new("message-consume-call", "messaging", 0.64),
            );
            self.push_handler_edge(
                arguments,
                callsite_id,
                scope_id,
                scope_name,
                &span,
                HeuristicTag::new("message-handler", "messaging", 0.58),
            );
        }

        if contains_any(&lower, &["subscribe"]) {
            let topic_id = self.push_extended_entity(
                EntityKind::Topic,
                &channel,
                scope_name,
                node,
                HeuristicTag::new("message-subscribe-call", "messaging", 0.64),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::SubscribesTo,
                &topic_id,
                &span,
                HeuristicTag::new("message-subscribe-call", "messaging", 0.64),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::Consumes,
                &topic_id,
                &span,
                HeuristicTag::new("message-subscribe-consume", "messaging", 0.58),
            );
            self.push_handler_edge(
                arguments,
                callsite_id,
                scope_id,
                scope_name,
                &span,
                HeuristicTag::new("message-subscribe-handler", "messaging", 0.58),
            );
        }

        if contains_any(
            &lower,
            &[
                "settimeout",
                "setinterval",
                "promise.all",
                "queue.add",
                "worker",
            ],
        ) {
            let kind = if contains_any(&lower, &["promise"]) {
                EntityKind::Promise
            } else {
                EntityKind::Task
            };
            let task_id = self.push_extended_entity(
                kind,
                callee_label,
                scope_name,
                node,
                HeuristicTag::new("async-spawn-call", "async", 0.60),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::Spawns,
                &task_id,
                &span,
                HeuristicTag::new("async-spawn-call", "async", 0.60),
            );
            if contains_any(&lower, &["settimeout", "setinterval"]) {
                if let Some(callback) = arguments.first().and_then(|argument| {
                    self.resolved_direct_executable_symbol(*argument, scope_id)
                }) {
                    self.push_edge_with(
                        scope_id,
                        RelationKind::Spawns,
                        &callback.id,
                        &span,
                        callback.exactness,
                        callback.confidence,
                    );
                    self.push_edge_with(
                        &task_id,
                        RelationKind::Handles,
                        &callback.id,
                        &span,
                        callback.exactness,
                        callback.confidence,
                    );
                }
            }
            if contains_any(&lower, &["promise"]) {
                self.push_exact_async_argument_spawns(arguments, scope_id, &span);
            }
        }
    }

    fn extract_persistence_call(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
        callee_label: &str,
        arguments: &[Node<'_>],
    ) {
        let lower = callee_label.to_ascii_lowercase();
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let migration_id = self.push_extended_entity(
            EntityKind::Migration,
            &format!("migration:{}", self.parsed.repo_relative_path),
            scope_name,
            node,
            HeuristicTag::new("migration-pattern", "schema", 0.58),
        );

        if contains_any(&lower, &["createtable", "altertable", "droptable"]) {
            if let Some(table) = first_string_argument(arguments, self.source) {
                let table_id = self.table_entity(&table, scope_name, node, "schema-table-call");
                self.push_extended_edge(
                    &migration_id,
                    RelationKind::Migrates,
                    &table_id,
                    &span,
                    HeuristicTag::new("schema-table-call", "schema", 0.64),
                );
                self.push_extended_edge(
                    &migration_id,
                    RelationKind::DependsOnSchema,
                    &table_id,
                    &span,
                    HeuristicTag::new("schema-table-call", "schema", 0.58),
                );
            }
        }

        if contains_any(&lower, &["addcolumn", "dropcolumn", "renamecolumn"]) {
            let table = string_argument(arguments, 0, self.source)
                .unwrap_or_else(|| "unknown_table".to_string());
            let column = string_argument(arguments, 1, self.source)
                .unwrap_or_else(|| "unknown_column".to_string());
            let column_id =
                self.column_entity(&table, &column, scope_name, node, "schema-column-call");
            self.push_extended_edge(
                &migration_id,
                RelationKind::AltersColumn,
                &column_id,
                &span,
                HeuristicTag::new("schema-column-call", "schema", 0.62),
            );
        }

        if table_column_method(&lower) {
            if let Some(column) = first_string_argument(arguments, self.source) {
                let column_id = self.column_entity(
                    "unknown_table",
                    &column,
                    scope_name,
                    node,
                    "table-column-builder",
                );
                self.push_extended_edge(
                    &migration_id,
                    RelationKind::AltersColumn,
                    &column_id,
                    &span,
                    HeuristicTag::new("table-column-builder", "schema", 0.54),
                );
            }
        }

        if contains_any(&lower, &["from", "select", "findmany", "findunique"]) {
            if let Some(table) = first_string_argument(arguments, self.source) {
                let head_id =
                    self.persistence_head(scope_id, scope_name, node, RelationKind::ReadsTable);
                let table_id = self.table_entity(&table, scope_name, node, "table-read-call");
                self.push_extended_edge(
                    &head_id,
                    RelationKind::ReadsTable,
                    &table_id,
                    &span,
                    HeuristicTag::new("table-read-call", "persistence", 0.58),
                );
            }
        }

        if contains_any(&lower, &["insert", "update", "delete", "into", "create"]) {
            if let Some(table) = first_string_argument(arguments, self.source) {
                let head_id =
                    self.persistence_head(scope_id, scope_name, node, RelationKind::WritesTable);
                let table_id = self.table_entity(&table, scope_name, node, "table-write-call");
                self.push_extended_edge(
                    &head_id,
                    RelationKind::WritesTable,
                    &table_id,
                    &span,
                    HeuristicTag::new("table-write-call", "persistence", 0.58),
                );
            }
        }

        for argument in arguments {
            let Some(sql) = string_literal_value(*argument, self.source) else {
                continue;
            };
            if let Some((relation, table)) = sql_table_relation(&sql) {
                let table_id = self.table_entity(&table, scope_name, *argument, "sql-string-table");
                let head_id = self.persistence_head(scope_id, scope_name, node, relation);
                self.push_extended_edge(
                    &head_id,
                    relation,
                    &table_id,
                    &span,
                    HeuristicTag::new("sql-string-table", "sql", 0.56),
                );
            }
            if let Some(column) = sql_altered_column(&sql) {
                let column_id = self.column_entity(
                    "unknown_table",
                    &column,
                    scope_name,
                    *argument,
                    "sql-string-column",
                );
                self.push_extended_edge(
                    &migration_id,
                    RelationKind::AltersColumn,
                    &column_id,
                    &span,
                    HeuristicTag::new("sql-string-column", "sql", 0.54),
                );
            }
        }
    }

    fn extract_test_call(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
        callee_label: &str,
        arguments: &[Node<'_>],
    ) {
        let lower = callee_label.to_ascii_lowercase();
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);

        if is_test_case_call(&lower) {
            let name = first_string_argument(arguments, self.source)
                .unwrap_or_else(|| callee_label.to_string());
            let test_id = self.push_extended_entity(
                EntityKind::TestCase,
                &name,
                scope_name,
                node,
                HeuristicTag::new("test-case-call", "jest-vitest", 0.70),
            );
            if let Some(test_file_id) = self.test_file_id.clone() {
                self.push_edge(&test_file_id, RelationKind::Contains, &test_id, &span);
                self.push_edge(&test_id, RelationKind::DefinedIn, &test_file_id, &span);
            }

            let behavior_id = self.push_expression_entity(
                &format!("behavior:{name}"),
                scope_name,
                node,
                "test-behavior",
                0.62,
            );
            self.push_extended_edge(
                &test_id,
                RelationKind::Tests,
                &behavior_id,
                &span,
                HeuristicTag::new("test-case-call", "jest-vitest", 0.70),
            );
            self.push_extended_edge(
                &test_id,
                RelationKind::Covers,
                &behavior_id,
                &span,
                HeuristicTag::new("test-case-call", "jest-vitest", 0.56),
            );

            let mut calls = Vec::new();
            collect_call_expression_nodes(node, &mut calls);
            for call in calls {
                if call.start_byte() == node.start_byte() && call.end_byte() == node.end_byte() {
                    continue;
                }
                let nested_label = call_callee_label(call, self.source);
                let nested_lower = nested_label.to_ascii_lowercase();
                let nested_args = call_argument_nodes(call);
                let nested_span = source_span_for_node(&self.parsed.repo_relative_path, call);
                if is_assert_call(&nested_lower) {
                    let target_id = nested_args
                        .first()
                        .copied()
                        .and_then(|arg| self.expression_entity(arg, scope_id, scope_name))
                        .unwrap_or_else(|| {
                            self.push_expression_entity(
                                &nested_label,
                                scope_name,
                                call,
                                "assertion-target",
                                0.55,
                            )
                        });
                    self.push_extended_edge(
                        &test_id,
                        RelationKind::Asserts,
                        &target_id,
                        &nested_span,
                        HeuristicTag::new("assertion-call", "jest-vitest", 0.66),
                    );
                }
                if is_mock_call(&nested_lower) {
                    let target_id = self.mock_target_entity(call, scope_name, &nested_args);
                    self.push_extended_edge(
                        &test_id,
                        RelationKind::Mocks,
                        &target_id,
                        &nested_span,
                        HeuristicTag::new("mock-call", "jest-vitest", 0.64),
                    );
                }
                if is_stub_call(&nested_lower) {
                    let target_id = self.stub_target_entity(call, scope_name, &nested_args);
                    self.push_extended_edge(
                        &test_id,
                        RelationKind::Stubs,
                        &target_id,
                        &nested_span,
                        HeuristicTag::new("stub-call", "jest-vitest", 0.64),
                    );
                }
                if is_fixture_call(&nested_lower) {
                    let target_id = self.push_expression_entity(
                        &nested_label,
                        scope_name,
                        call,
                        "fixture-target",
                        0.56,
                    );
                    self.push_extended_edge(
                        &test_id,
                        RelationKind::FixturesFor,
                        &target_id,
                        &nested_span,
                        HeuristicTag::new("fixture-call", "jest-vitest", 0.56),
                    );
                }
                if !is_test_framework_callee_label(&nested_lower) {
                    if let Some(name) = generic_callee_symbol_name(&nested_label) {
                        if let Some(symbol) = self.resolve_symbol(scope_id, &name) {
                            if is_proof_grade_exactness(symbol.exactness) {
                                self.push_edge_with(
                                    &test_id,
                                    RelationKind::Tests,
                                    &symbol.id,
                                    &nested_span,
                                    symbol.exactness,
                                    symbol.confidence,
                                );
                            }
                        }
                    }
                }
            }
        }

        if let Some(test_file_id) = self.test_file_id.clone() {
            if is_mock_call(&lower) {
                let target_id = self.mock_target_entity(node, scope_name, arguments);
                self.push_extended_edge(
                    &test_file_id,
                    RelationKind::Mocks,
                    &target_id,
                    &span,
                    HeuristicTag::new("mock-call", "jest-vitest", 0.64),
                );
            }
            if is_stub_call(&lower) {
                let target_id = self.stub_target_entity(node, scope_name, arguments);
                self.push_extended_edge(
                    &test_file_id,
                    RelationKind::Stubs,
                    &target_id,
                    &span,
                    HeuristicTag::new("stub-call", "jest-vitest", 0.64),
                );
            }
            if is_fixture_call(&lower) {
                let target_id = self.push_expression_entity(
                    callee_label,
                    scope_name,
                    node,
                    "fixture-target",
                    0.56,
                );
                self.push_extended_edge(
                    &test_file_id,
                    RelationKind::FixturesFor,
                    &target_id,
                    &span,
                    HeuristicTag::new("fixture-call", "jest-vitest", 0.56),
                );
            }
        }
    }

    fn extract_new_expression(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        self.extract_constructor_call(node, scope_id, scope_name);

        let label = expression_label(node, self.source);
        let lower = label.to_ascii_lowercase();
        if !contains_any(&lower, &["promise", "worker", "task", "job"]) {
            return;
        }
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let kind = if lower.contains("promise") {
            EntityKind::Promise
        } else {
            EntityKind::Task
        };
        let head_id = self.executable_or_task_head(scope_id, scope_name, node);
        let task_name = synthetic_source_identity(
            if matches!(kind, EntityKind::Promise) {
                "promise_expr"
            } else {
                "task_expr"
            },
            node,
            &label,
        );
        let task_id = self.push_extended_entity(
            kind,
            &task_name,
            scope_name,
            node,
            HeuristicTag::new("new-async-work", "async", 0.58),
        );
        self.push_extended_edge(
            &head_id,
            RelationKind::Spawns,
            &task_id,
            &span,
            HeuristicTag::new("new-async-work", "async", 0.58),
        );
    }

    fn extract_constructor_call(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let callee_node = node
            .child_by_field_name("constructor")
            .or_else(|| node.child_by_field_name("function"))
            .or_else(|| first_named_child(node));
        let callee_label = callee_node
            .and_then(|callee| {
                node_text(callee, self.source).map(|text| compact_extracted_label(&text, callee))
            })
            .unwrap_or_else(|| "unknown_constructor".to_string());
        let callsite_name = format!("new:{callee_label}");
        let callsite_id = self.push_entity(
            EntityKind::CallSite,
            &callsite_name,
            &qualify(
                scope_name,
                &format!("{callsite_name}@{}", node.start_byte()),
            ),
            span.clone(),
        );
        self.scope_parents
            .insert(callsite_id.clone(), scope_id.to_string());
        self.push_edge(scope_id, RelationKind::Contains, &callsite_id, &span);
        self.push_edge(&callsite_id, RelationKind::DefinedIn, scope_id, &span);

        let (callee_id, exactness, confidence) =
            self.constructor_entity(callee_node, scope_id, scope_name);
        self.push_edge_with(
            scope_id,
            RelationKind::Calls,
            &callee_id,
            &span,
            exactness,
            confidence,
        );
        self.push_edge_with(
            &callsite_id,
            RelationKind::Callee,
            &callee_id,
            &span,
            exactness,
            confidence,
        );
    }

    fn constructor_entity(
        &mut self,
        callee_node: Option<Node<'_>>,
        scope_id: &str,
        scope_name: &str,
    ) -> (String, Exactness, f64) {
        let Some(callee_node) = callee_node else {
            let id = self.push_reference_entity(
                EntityKind::Constructor,
                "unknown_constructor",
                scope_name,
                self.parsed.tree.root_node(),
                "unresolved-constructor",
                0.4,
            );
            return (id, Exactness::StaticHeuristic, 0.4);
        };

        if callee_node.kind() == "identifier" {
            if let Some(name) = node_text(callee_node, self.source) {
                if let Some(symbol) = self.resolve_symbol(scope_id, &name) {
                    if self
                        .entity_kinds
                        .get(&symbol.id)
                        .is_some_and(|kind| *kind == EntityKind::Class)
                    {
                        let span =
                            source_span_for_node(&self.parsed.repo_relative_path, callee_node);
                        self.push_edge_with(
                            scope_id,
                            RelationKind::Instantiates,
                            &symbol.id,
                            &span,
                            symbol.exactness,
                            symbol.confidence,
                        );
                        if let Some(constructor_id) = self.constructor_for_class(&symbol.id) {
                            return (constructor_id, symbol.exactness, symbol.confidence);
                        }
                    }
                }
                let id = self.push_reference_entity(
                    EntityKind::Constructor,
                    &name,
                    scope_name,
                    callee_node,
                    "unresolved-constructor",
                    0.55,
                );
                return (id, Exactness::StaticHeuristic, 0.55);
            }
        }

        let label = expression_label(callee_node, self.source);
        let id = self.push_reference_entity(
            EntityKind::Constructor,
            &label,
            scope_name,
            callee_node,
            "unresolved-constructor",
            0.55,
        );
        (id, Exactness::StaticHeuristic, 0.55)
    }

    fn constructor_for_class(&self, class_id: &str) -> Option<String> {
        self.entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Constructor
                    && self
                        .scope_parents
                        .get(&entity.id)
                        .is_some_and(|parent_id| parent_id == class_id)
            })
            .map(|entity| entity.id.clone())
    }

    fn extract_await_expression(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let label = expression_label(node, self.source);
        let head_id = self.executable_or_task_head(scope_id, scope_name, node);
        if let Some(callee) = self.awaited_direct_call_symbol(node, scope_id) {
            self.push_edge_with(
                &head_id,
                RelationKind::Awaits,
                &callee.id,
                &span,
                callee.exactness,
                callee.confidence,
            );
        }
        let promise_name = synthetic_source_identity("await_expr", node, &label);
        let promise_id = self.push_extended_entity(
            EntityKind::Promise,
            &promise_name,
            scope_name,
            node,
            HeuristicTag::new("await-expression", "async", 0.62),
        );
        self.push_extended_edge(
            &head_id,
            RelationKind::Awaits,
            &promise_id,
            &span,
            HeuristicTag::new("await-expression", "async", 0.62),
        );
    }

    fn push_handler_edge(
        &mut self,
        arguments: &[Node<'_>],
        head_id: &str,
        scope_id: &str,
        scope_name: &str,
        span: &SourceSpan,
        tag: HeuristicTag<'_>,
    ) {
        for argument in arguments.iter().skip(1) {
            if let Some(handler_id) = self.handler_entity(*argument, scope_id, scope_name) {
                self.push_extended_edge(head_id, RelationKind::Handles, &handler_id, span, tag);
            }
        }
    }

    fn handler_entity(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
    ) -> Option<String> {
        if node.kind() == "identifier" {
            let name = node_text(node, self.source)?;
            return Some(
                self.resolve_symbol(scope_id, &name)
                    .map(|symbol| symbol.id)
                    .unwrap_or_else(|| {
                        self.push_extended_entity(
                            EntityKind::Function,
                            &name,
                            scope_name,
                            node,
                            HeuristicTag::new("handler-reference", "framework", 0.55),
                        )
                    }),
            );
        }
        if matches!(
            node.kind(),
            "arrow_function" | "function" | "function_expression"
        ) {
            return Some(self.push_extended_entity(
                EntityKind::Function,
                &format!("handler@{}", node.start_byte()),
                scope_name,
                node,
                HeuristicTag::new("inline-handler", "framework", 0.55),
            ));
        }
        None
    }

    fn resolved_direct_executable_symbol(
        &self,
        node: Node<'_>,
        scope_id: &str,
    ) -> Option<SymbolRef> {
        if node.kind() != "identifier" {
            return None;
        }
        let name = node_text(node, self.source)?;
        let symbol = self.resolve_symbol(scope_id, &name)?;
        if !is_proof_grade_exactness(symbol.exactness) {
            return None;
        }
        if !self.entity_kinds.get(&symbol.id).is_some_and(|kind| {
            matches!(
                kind,
                EntityKind::Function | EntityKind::Method | EntityKind::Constructor
            )
        }) {
            return None;
        }
        Some(symbol)
    }

    fn direct_call_symbol(&self, node: Node<'_>, scope_id: &str) -> Option<SymbolRef> {
        if node.kind() != "call_expression" {
            return None;
        }
        let callee = node.child_by_field_name("function")?;
        self.resolved_direct_executable_symbol(callee, scope_id)
    }

    fn awaited_direct_call_symbol(&self, node: Node<'_>, scope_id: &str) -> Option<SymbolRef> {
        let awaited = first_named_child(node)?;
        self.direct_call_symbol(awaited, scope_id)
    }

    fn push_exact_async_argument_spawns(
        &mut self,
        arguments: &[Node<'_>],
        scope_id: &str,
        span: &SourceSpan,
    ) {
        for argument in arguments {
            let mut calls = Vec::new();
            collect_call_expression_nodes(*argument, &mut calls);
            for call in calls {
                if let Some(callee) = self.direct_call_symbol(call, scope_id) {
                    self.push_edge_with(
                        scope_id,
                        RelationKind::Spawns,
                        &callee.id,
                        span,
                        callee.exactness,
                        callee.confidence,
                    );
                }
            }
        }
    }

    fn table_entity(
        &mut self,
        table: &str,
        scope_name: &str,
        node: Node<'_>,
        pattern: &str,
    ) -> String {
        self.push_extended_entity(
            EntityKind::Table,
            table,
            scope_name,
            node,
            HeuristicTag::new(pattern, "persistence", 0.58),
        )
    }

    fn column_entity(
        &mut self,
        table: &str,
        column: &str,
        scope_name: &str,
        node: Node<'_>,
        pattern: &str,
    ) -> String {
        self.push_extended_entity(
            EntityKind::Column,
            &format!("{table}.{column}"),
            scope_name,
            node,
            HeuristicTag::new(pattern, "persistence", 0.56),
        )
    }

    fn persistence_head(
        &mut self,
        scope_id: &str,
        scope_name: &str,
        node: Node<'_>,
        relation: RelationKind,
    ) -> String {
        if let Some(kind) = self.entity_kinds.get(scope_id).copied() {
            if relation_allows(relation, kind, EntityKind::Table) {
                return scope_id.to_string();
            }
        }
        self.push_extended_entity(
            EntityKind::Migration,
            &format!("migration:{}", self.parsed.repo_relative_path),
            scope_name,
            node,
            HeuristicTag::new("persistence-head-fallback", "persistence", 0.52),
        )
    }

    fn executable_or_task_head(
        &mut self,
        scope_id: &str,
        scope_name: &str,
        node: Node<'_>,
    ) -> String {
        if let Some(kind) = self.entity_kinds.get(scope_id).copied() {
            if relation_allows(RelationKind::Awaits, kind, EntityKind::Promise) {
                return scope_id.to_string();
            }
        }
        self.push_extended_entity(
            EntityKind::Task,
            &format!("task:{}", self.parsed.repo_relative_path),
            scope_name,
            node,
            HeuristicTag::new("async-head-fallback", "async", 0.52),
        )
    }

    fn mock_target_entity(
        &mut self,
        node: Node<'_>,
        scope_name: &str,
        arguments: &[Node<'_>],
    ) -> String {
        if let Some(module_name) = first_string_argument(arguments, self.source) {
            return self.push_extended_entity(
                EntityKind::Dependency,
                &module_name,
                scope_name,
                node,
                HeuristicTag::new("mock-target", "jest-vitest", 0.58),
            );
        }
        self.push_expression_entity(
            &expression_label(node, self.source),
            scope_name,
            node,
            "mock-target",
            0.55,
        )
    }

    fn stub_target_entity(
        &mut self,
        node: Node<'_>,
        scope_name: &str,
        arguments: &[Node<'_>],
    ) -> String {
        if let Some(name) = first_string_argument(arguments, self.source) {
            return self.push_extended_entity(
                EntityKind::EnvVar,
                &name,
                scope_name,
                node,
                HeuristicTag::new("stub-target", "jest-vitest", 0.58),
            );
        }
        self.push_expression_entity(
            &expression_label(node, self.source),
            scope_name,
            node,
            "stub-target",
            0.55,
        )
    }

    fn extract_return(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let return_name = format!("return@{}", node.start_byte());
        let return_id = self.push_entity(
            EntityKind::ReturnSite,
            &return_name,
            &qualify(scope_name, &return_name),
            span.clone(),
        );
        self.push_edge(scope_id, RelationKind::Contains, &return_id, &span);
        self.push_edge(&return_id, RelationKind::DefinedIn, scope_id, &span);
        self.push_edge(scope_id, RelationKind::Returns, &return_id, &span);
        self.push_edge(&return_id, RelationKind::ReturnsTo, scope_id, &span);
        self.return_sites_by_scope
            .entry(scope_id.to_string())
            .or_default()
            .push(return_id.clone());

        if let Some(value) = first_return_value(node) {
            if let Some(source_id) = self.expression_entity(value, scope_id, scope_name) {
                let value_span = source_span_for_node(&self.parsed.repo_relative_path, value);
                self.push_edge(&source_id, RelationKind::FlowsTo, &return_id, &value_span);
            }
            self.extract_reads_from_expression(value, scope_id, None);
            self.extract_table_sink_return(value, scope_id, scope_name);
        }
    }

    fn extract_return_value_assignment_flow(
        &mut self,
        value: Node<'_>,
        target: &SymbolRef,
        span: &SourceSpan,
        scope_id: &str,
        scope_name: &str,
    ) {
        if value.kind() != "call_expression" || !is_proof_grade_exactness(target.exactness) {
            return;
        }
        let callee_node = value.child_by_field_name("function");
        let (callee_id, exactness, confidence) =
            self.callee_entity(callee_node, scope_id, scope_name);
        if !is_proof_grade_exactness(exactness) {
            return;
        }
        let Some(return_sites) = self.return_sites_by_scope.get(&callee_id).cloned() else {
            return;
        };
        for return_site_id in return_sites {
            self.push_edge_with(
                &return_site_id,
                RelationKind::FlowsTo,
                &target.id,
                span,
                exactness,
                confidence.min(target.confidence),
            );
        }
    }

    fn extract_table_sink_return(&mut self, value: Node<'_>, scope_id: &str, scope_name: &str) {
        if value.kind() != "identifier" || !looks_like_persistence_writer_name(scope_name) {
            return;
        }
        let Some(name) = node_text(value, self.source) else {
            return;
        };
        if !looks_like_table_constant_name(&name) {
            return;
        }
        let Some(table_id) = self.resolve_table_symbol(scope_id, &name) else {
            return;
        };
        let span = source_span_for_node(&self.parsed.repo_relative_path, value);
        self.push_edge(scope_id, RelationKind::Writes, &table_id, &span);
    }

    fn extract_reads_from_expression(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        skip_id: Option<&str>,
    ) {
        let mut identifiers = Vec::new();
        collect_identifier_nodes_capped(
            node,
            &mut identifiers,
            expression_read_identifier_cap(
                &self.parsed.repo_relative_path,
                self.source.len(),
                node,
            ),
        );
        for identifier in identifiers {
            let Some(name) = node_text(identifier, self.source) else {
                continue;
            };
            let Some(symbol) = self.resolve_or_reference_symbol(scope_id, &name, identifier) else {
                continue;
            };
            if skip_id == Some(symbol.id.as_str()) {
                continue;
            }
            let span = source_span_for_node(&self.parsed.repo_relative_path, identifier);
            self.push_edge_with(
                scope_id,
                RelationKind::Reads,
                &symbol.id,
                &span,
                symbol.exactness,
                symbol.confidence,
            );
        }
    }

    fn assignment_target_entity(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
    ) -> Option<SymbolRef> {
        if node.kind() == "identifier" {
            let name = node_text(node, self.source)?;
            return Some(self.resolve_symbol(scope_id, &name).unwrap_or_else(|| {
                let id = self.push_reference_entity(
                    EntityKind::LocalVariable,
                    &name,
                    scope_name,
                    node,
                    "unresolved-write-target",
                    0.55,
                );
                SymbolRef {
                    id,
                    exactness: Exactness::StaticHeuristic,
                    confidence: 0.55,
                }
            }));
        }

        let id = self.push_expression_entity(
            &expression_label(node, self.source),
            scope_name,
            node,
            "assignment-target",
            0.85,
        );
        Some(SymbolRef {
            id,
            exactness: Exactness::StaticHeuristic,
            confidence: 0.85,
        })
    }

    fn expression_entity(
        &mut self,
        node: Node<'_>,
        scope_id: &str,
        scope_name: &str,
    ) -> Option<String> {
        if node.kind() == "identifier" {
            let name = node_text(node, self.source)?;
            return self
                .resolve_symbol(scope_id, &name)
                .map(|symbol| symbol.id)
                .or_else(|| {
                    Some(self.push_reference_entity(
                        EntityKind::LocalVariable,
                        &name,
                        scope_name,
                        node,
                        "unresolved-read-reference",
                        0.55,
                    ))
                });
        }

        Some(self.push_expression_entity(
            &expression_label(node, self.source),
            scope_name,
            node,
            "expression",
            0.85,
        ))
    }

    fn callee_entity(
        &mut self,
        callee_node: Option<Node<'_>>,
        scope_id: &str,
        scope_name: &str,
    ) -> (String, Exactness, f64) {
        let Some(callee_node) = callee_node else {
            let id = self.push_reference_entity(
                EntityKind::Function,
                "unknown_callee",
                scope_name,
                self.parsed.tree.root_node(),
                "unresolved-callee",
                0.4,
            );
            return (id, Exactness::StaticHeuristic, 0.4);
        };

        if callee_node.kind() == "identifier" {
            if let Some(name) = node_text(callee_node, self.source) {
                if let Some(symbol) = self.resolve_symbol(scope_id, &name) {
                    return (symbol.id, Exactness::ParserVerified, 1.0);
                }
                let id = self.push_reference_entity(
                    EntityKind::Function,
                    &name,
                    scope_name,
                    callee_node,
                    "unresolved-callee",
                    0.55,
                );
                return (id, Exactness::StaticHeuristic, 0.55);
            }
        }

        let label = expression_label(callee_node, self.source);
        let kind = if label.contains('.') {
            EntityKind::Method
        } else {
            EntityKind::Function
        };
        let id = self.push_reference_entity(
            kind,
            &label,
            scope_name,
            callee_node,
            "unresolved-callee",
            0.55,
        );
        (id, Exactness::StaticHeuristic, 0.55)
    }

    fn resolve_or_reference_symbol(
        &mut self,
        scope_id: &str,
        name: &str,
        node: Node<'_>,
    ) -> Option<SymbolRef> {
        self.resolve_symbol(scope_id, name).or_else(|| {
            let id = self.push_reference_entity(
                EntityKind::LocalVariable,
                name,
                scope_id,
                node,
                "unresolved-read-reference",
                0.55,
            );
            Some(SymbolRef {
                id,
                exactness: Exactness::StaticHeuristic,
                confidence: 0.55,
            })
        })
    }

    fn resolve_symbol(&self, scope_id: &str, name: &str) -> Option<SymbolRef> {
        let mut current = Some(scope_id);
        while let Some(scope) = current {
            if self
                .ambiguous_symbols_by_scope
                .get(scope)
                .is_some_and(|symbols| symbols.contains(name))
            {
                return None;
            }
            if let Some(symbol) = self
                .symbols_by_scope
                .get(scope)
                .and_then(|symbols| symbols.get(name))
            {
                return Some(symbol.clone());
            }
            current = self.scope_parents.get(scope).map(String::as_str);
        }
        None
    }

    fn resolve_table_symbol(&self, scope_id: &str, name: &str) -> Option<String> {
        let mut current = Some(scope_id);
        while let Some(scope) = current {
            if let Some(id) = self
                .table_symbols_by_scope
                .get(scope)
                .and_then(|symbols| symbols.get(name))
            {
                return Some(id.clone());
            }
            current = self.scope_parents.get(scope).map(String::as_str);
        }
        None
    }

    fn register_table_symbol(&mut self, scope_id: &str, name: &str, id: &str) {
        self.table_symbols_by_scope
            .entry(scope_id.to_string())
            .or_default()
            .insert(name.to_string(), id.to_string());
    }

    fn register_symbol_with(
        &mut self,
        scope_id: &str,
        name: &str,
        id: &str,
        exactness: Exactness,
        confidence: f64,
    ) {
        let symbols = self
            .symbols_by_scope
            .entry(scope_id.to_string())
            .or_default();
        if symbols.get(name).is_some_and(|existing| existing.id != id) {
            self.ambiguous_symbols_by_scope
                .entry(scope_id.to_string())
                .or_default()
                .insert(name.to_string());
            return;
        }
        symbols.insert(
            name.to_string(),
            SymbolRef {
                id: id.to_string(),
                exactness,
                confidence,
            },
        );
    }

    fn register_parser_observed_import_binding(
        &mut self,
        scope_id: &str,
        binding_id: &str,
        binding: &ImportBinding,
    ) {
        // Only Python relies on parser-observed import bindings to suppress
        // bogus unresolved-CALL escalation (its from-import aliases are not
        // otherwise resolvable in the local symbol table). JS/TS/JSX/TSX are
        // deliberately EXCLUDED: for those, a call to a VALID imported name is
        // already suppressed downstream by the whole-graph "candidates-exist"
        // definition lookup, so registering the binding here would only
        // suppress calls to a DANGLING import (one whose target the source
        // module does not export) — reopening the Q4 forward-hallucination
        // blind spot that validate-edit must catch.
        if self.parsed.language != SourceLanguage::Python {
            return;
        }
        if !looks_like_identifier(&binding.local_name) {
            return;
        }
        self.register_symbol_with(
            scope_id,
            &binding.local_name,
            binding_id,
            Exactness::StaticHeuristic,
            0.5,
        );
    }

    fn push_reference_entity(
        &mut self,
        kind: EntityKind,
        name: &str,
        scope_name: &str,
        node: Node<'_>,
        reason: &str,
        confidence: f64,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let qualified_name = if is_static_executable_reference(kind) {
            format!("static_reference:{name}")
        } else {
            qualify(scope_name, name)
        };
        let id = self.push_entity_with(
            kind,
            name,
            &qualified_name,
            span,
            "tree-sitter-static-heuristic",
            confidence,
        );
        if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
            entity
                .metadata
                .insert("heuristic_reason".to_string(), reason.into());
        }
        self.annotate_entity_source_role(&id, kind, name, &qualified_name, Some(node));
        id
    }

    fn push_extended_entity(
        &mut self,
        kind: EntityKind,
        name: &str,
        scope_name: &str,
        node: Node<'_>,
        tag: HeuristicTag<'_>,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity_with(
            kind,
            name,
            &qualify(scope_name, name),
            span,
            "tree-sitter-extended-heuristic",
            tag.confidence,
        );
        if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
            entity
                .metadata
                .insert("heuristic_pattern".to_string(), tag.pattern.into());
            entity
                .metadata
                .insert("framework".to_string(), tag.framework.into());
            entity.metadata.insert("phase".to_string(), "07".into());
        }
        self.annotate_entity_source_role(&id, kind, name, &qualify(scope_name, name), Some(node));
        id
    }

    fn push_expression_entity(
        &mut self,
        _label: &str,
        scope_name: &str,
        node: Node<'_>,
        reason: &str,
        confidence: f64,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let compact_name = format!("expr@{}", node.start_byte());
        let id = self.push_entity_with(
            EntityKind::Expression,
            &compact_name,
            &qualify(scope_name, &compact_name),
            span,
            "tree-sitter-basic",
            confidence,
        );
        if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
            entity
                .metadata
                .insert("expression_reason".to_string(), reason.into());
        }
        self.annotate_entity_source_role(
            &id,
            EntityKind::Expression,
            &compact_name,
            &qualify(scope_name, &compact_name),
            Some(node),
        );
        id
    }

    fn push_scoped_entity(
        &mut self,
        scope_id: &str,
        kind: EntityKind,
        name: &str,
        qualified_name: &str,
        node: Node<'_>,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity(kind, name, qualified_name, span.clone());
        self.annotate_entity_source_role(&id, kind, name, qualified_name, Some(node));
        let (exactness, confidence) = if node_has_error_or_missing_descendant(node) {
            if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
                annotate_untrusted_syntax_entity(
                    entity,
                    "declaration node contains tree-sitter ERROR or MISSING descendant",
                );
            }
            (Exactness::StaticHeuristic, 0.45)
        } else {
            (Exactness::ParserVerified, 1.0)
        };
        if is_scope_kind(kind) {
            self.scope_parents.insert(id.clone(), scope_id.to_string());
        }
        self.register_symbol_with(scope_id, name, &id, exactness, confidence);
        self.push_edge_with(
            scope_id,
            RelationKind::Contains,
            &id,
            &span,
            exactness,
            confidence,
        );
        self.push_edge_with(
            &id,
            RelationKind::DefinedIn,
            scope_id,
            &span,
            exactness,
            confidence,
        );
        let declaration_relation = if matches!(
            kind,
            EntityKind::Class
                | EntityKind::Interface
                | EntityKind::Function
                | EntityKind::Method
                | EntityKind::Constructor
        ) {
            RelationKind::Defines
        } else {
            RelationKind::Declares
        };
        self.push_edge_with(
            scope_id,
            declaration_relation,
            &id,
            &span,
            exactness,
            confidence,
        );
        id
    }

    fn push_table_constant_entity(
        &mut self,
        scope_id: &str,
        name: &str,
        qualified_name: &str,
        node: Node<'_>,
    ) -> String {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity(EntityKind::Table, name, qualified_name, span.clone());
        self.register_table_symbol(scope_id, name, &id);
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
        self.push_edge(scope_id, RelationKind::Declares, &id, &span);
        if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
            entity
                .metadata
                .insert("persistence_pattern".to_string(), "table_constant".into());
            entity.metadata.insert("phase".to_string(), "30".into());
        }
        id
    }

    fn push_entity(
        &mut self,
        kind: EntityKind,
        name: &str,
        qualified_name: &str,
        span: SourceSpan,
    ) -> String {
        self.push_entity_with(kind, name, qualified_name, span, "tree-sitter-basic", 1.0)
    }

    fn push_entity_with(
        &mut self,
        kind: EntityKind,
        name: &str,
        qualified_name: &str,
        span: SourceSpan,
        created_from: &str,
        confidence: f64,
    ) -> String {
        let signature = if created_from == "tree-sitter-static-heuristic" {
            format!("{qualified_name}@static-reference")
        } else {
            format!(
                "{}@{}:{}-{}:{}",
                qualified_name,
                span.start_line,
                span.start_column.unwrap_or(1),
                span.end_line,
                span.end_column.unwrap_or(1)
            )
        };
        let id = stable_entity_id_for_kind(
            &self.parsed.repo_relative_path,
            kind,
            qualified_name,
            Some(&signature),
        );
        self.entity_kinds.insert(id.clone(), kind);
        if !self.entity_indices.contains_key(&id) {
            let role = source_role_annotation(
                self.parsed.language,
                &self.parsed.repo_relative_path,
                kind,
                name,
                qualified_name,
                None,
                self.source,
            );
            let mut entity = Entity {
                id: id.clone(),
                kind,
                name: name.to_string(),
                qualified_name: qualified_name.to_string(),
                repo_relative_path: self.parsed.repo_relative_path.clone(),
                source_span: Some(span),
                content_hash: None,
                file_hash: Some(self.file_hash.clone()),
                created_from: created_from.to_string(),
                confidence,
                metadata: Default::default(),
            };
            entity
                .metadata
                .insert("source_role".to_string(), role.role.as_str().into());
            entity
                .metadata
                .insert("source_role_reason".to_string(), role.reason.into());
            entity
                .metadata
                .insert("source_role_source".to_string(), role.source.into());
            self.entity_source_roles.insert(id.clone(), role.role);
            self.entity_indices.insert(id.clone(), self.entities.len());
            self.entities.push(entity);
        }
        id
    }

    fn annotate_entity_source_role(
        &mut self,
        id: &str,
        kind: EntityKind,
        name: &str,
        qualified_name: &str,
        node: Option<Node<'_>>,
    ) {
        let role = source_role_annotation(
            self.parsed.language,
            &self.parsed.repo_relative_path,
            kind,
            name,
            qualified_name,
            node,
            self.source,
        );
        self.entity_source_roles.insert(id.to_string(), role.role);
        if let Some(index) = self.entity_indices.get(id).copied() {
            if let Some(entity) = self.entities.get_mut(index) {
                entity
                    .metadata
                    .insert("source_role".to_string(), role.role.as_str().into());
                entity
                    .metadata
                    .insert("source_role_reason".to_string(), role.reason.into());
                entity
                    .metadata
                    .insert("source_role_source".to_string(), role.source.into());
            }
        }
    }

    fn push_edge(
        &mut self,
        head_id: &str,
        relation: RelationKind,
        tail_id: &str,
        span: &SourceSpan,
    ) {
        self.push_edge_with(
            head_id,
            relation,
            tail_id,
            span,
            Exactness::ParserVerified,
            1.0,
        );
    }

    fn push_edge_with(
        &mut self,
        head_id: &str,
        relation: RelationKind,
        tail_id: &str,
        span: &SourceSpan,
        exactness: Exactness,
        confidence: f64,
    ) {
        self.push_edge_with_extractor(
            head_id,
            relation,
            tail_id,
            span,
            EdgeAnnotation {
                exactness,
                confidence,
                extractor: "tree-sitter-basic",
                heuristic: None,
            },
        );
    }

    fn push_extended_edge(
        &mut self,
        head_id: &str,
        relation: RelationKind,
        tail_id: &str,
        span: &SourceSpan,
        tag: HeuristicTag<'_>,
    ) {
        self.push_edge_with_extractor(
            head_id,
            relation,
            tail_id,
            span,
            EdgeAnnotation {
                exactness: Exactness::StaticHeuristic,
                confidence: tag.confidence,
                extractor: "tree-sitter-extended-heuristic",
                heuristic: Some(tag),
            },
        );
    }

    fn push_edge_with_extractor(
        &mut self,
        head_id: &str,
        relation: RelationKind,
        tail_id: &str,
        span: &SourceSpan,
        annotation: EdgeAnnotation<'_>,
    ) {
        let Some(head_kind) = self.entity_kinds.get(head_id).copied() else {
            return;
        };
        let Some(tail_kind) = self.entity_kinds.get(tail_id).copied() else {
            return;
        };
        if !relation_allows(relation, head_kind, tail_kind) {
            return;
        }

        let id = stable_edge_id(head_id, relation, tail_id, span);
        if !self.edge_ids.insert(id.clone()) {
            return;
        }

        let head_role = self
            .entity_source_roles
            .get(head_id)
            .copied()
            .unwrap_or(EvidenceRole::Unknown);
        let tail_role = self
            .entity_source_roles
            .get(tail_id)
            .copied()
            .unwrap_or(EvidenceRole::Unknown);
        let role = edge_role_annotation(relation, span, head_role, tail_role);
        self.edges.push(Edge {
            id,
            head_id: head_id.to_string(),
            relation,
            tail_id: tail_id.to_string(),
            source_span: span.clone(),
            repo_commit: None,
            file_hash: Some(self.file_hash.clone()),
            extractor: annotation.extractor.to_string(),
            confidence: annotation.confidence,
            exactness: annotation.exactness,
            edge_class: if annotation.exactness == Exactness::StaticHeuristic {
                EdgeClass::BaseHeuristic
            } else {
                EdgeClass::BaseExact
            },
            context: edge_context_for_role(role.role),
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        });
        if let Some(edge) = self.edges.last_mut() {
            edge.metadata
                .insert("source_role".to_string(), role.role.as_str().into());
            edge.metadata
                .insert("evidence_role".to_string(), role.role.as_str().into());
            edge.metadata
                .insert("classification_reason".to_string(), role.reason.into());
            edge.metadata
                .insert("classification_source".to_string(), role.source.into());
            edge.metadata
                .insert("head_source_role".to_string(), head_role.as_str().into());
            edge.metadata
                .insert("tail_source_role".to_string(), tail_role.as_str().into());
        }
        if annotation.exactness == Exactness::StaticHeuristic {
            if let Some(edge) = self.edges.last_mut() {
                edge.metadata.insert("heuristic".to_string(), true.into());
                edge.metadata.insert(
                    "resolution".to_string(),
                    "unresolved_static_heuristic".into(),
                );
            }
        }
        if let Some(tag) = annotation.heuristic {
            if let Some(edge) = self.edges.last_mut() {
                edge.metadata.insert("phase".to_string(), "07".into());
                edge.metadata.insert("heuristic".to_string(), true.into());
                edge.metadata
                    .insert("pattern".to_string(), tag.pattern.into());
                edge.metadata
                    .insert("framework".to_string(), tag.framework.into());
            }
        }
    }
}

fn source_span_for_node(repo_relative_path: &str, node: Node<'_>) -> SourceSpan {
    SyntaxNodeRef::from_node(repo_relative_path, node).source_span
}

#[derive(Debug, Clone)]
struct RecoveredDeclaration {
    kind: EntityKind,
    name: String,
    span: SourceSpan,
}

fn recoverable_declarations_from_source(
    language: SourceLanguage,
    repo_relative_path: &str,
    source: &str,
) -> Vec<RecoveredDeclaration> {
    let mut recovered = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || starts_with_comment_or_preprocessor(trimmed) {
            continue;
        }
        if let Some((kind, name, column)) = recoverable_declaration_on_line(language, line, trimmed)
        {
            let start_line = line_index as u32 + 1;
            let start_column = column as u32 + 1;
            let end_column = (line.len() as u32 + 1).max(start_column + 1);
            recovered.push(RecoveredDeclaration {
                kind,
                name,
                span: SourceSpan::with_columns(
                    repo_relative_path,
                    start_line,
                    start_column,
                    start_line,
                    end_column,
                ),
            });
        }
    }
    recovered
}

fn recoverable_declaration_on_line(
    language: SourceLanguage,
    line: &str,
    trimmed: &str,
) -> Option<(EntityKind, String, usize)> {
    match language {
        SourceLanguage::JavaScript
        | SourceLanguage::Jsx
        | SourceLanguage::TypeScript
        | SourceLanguage::Tsx => recover_js_family_declaration(line, trimmed),
        SourceLanguage::Python => {
            recover_keyword_declaration(line, trimmed, "def ", EntityKind::Function)
                .or_else(|| recover_keyword_declaration(line, trimmed, "class ", EntityKind::Class))
        }
        SourceLanguage::Ruby => {
            recover_keyword_declaration(line, trimmed, "def ", EntityKind::Function)
                .or_else(|| recover_keyword_declaration(line, trimmed, "class ", EntityKind::Class))
                .or_else(|| {
                    recover_keyword_declaration(line, trimmed, "module ", EntityKind::Module)
                })
        }
        SourceLanguage::Rust => {
            let stripped = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
            recover_keyword_declaration(line, stripped, "fn ", EntityKind::Function)
                .or_else(|| recover_keyword_declaration(line, stripped, "mod ", EntityKind::Module))
                .or_else(|| {
                    recover_keyword_declaration(line, stripped, "struct ", EntityKind::Class)
                })
                .or_else(|| recover_keyword_declaration(line, stripped, "enum ", EntityKind::Enum))
                .or_else(|| {
                    recover_keyword_declaration(line, stripped, "trait ", EntityKind::Trait)
                })
        }
        SourceLanguage::Go => recover_go_declaration(line, trimmed),
        SourceLanguage::C | SourceLanguage::Cpp => {
            recover_c_like_function_declaration(line, trimmed)
        }
        SourceLanguage::Java | SourceLanguage::CSharp => {
            recover_keyword_declaration(line, trimmed, "class ", EntityKind::Class).or_else(|| {
                recover_keyword_declaration(line, trimmed, "interface ", EntityKind::Interface)
            })
        }
        SourceLanguage::Php => {
            recover_keyword_declaration(line, trimmed, "function ", EntityKind::Function)
                .or_else(|| recover_keyword_declaration(line, trimmed, "class ", EntityKind::Class))
                .or_else(|| {
                    recover_keyword_declaration(line, trimmed, "interface ", EntityKind::Interface)
                })
        }
    }
}

fn recover_js_family_declaration(line: &str, trimmed: &str) -> Option<(EntityKind, String, usize)> {
    let mut stripped = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    stripped = stripped.strip_prefix("default ").unwrap_or(stripped);
    stripped = stripped.strip_prefix("async ").unwrap_or(stripped);
    recover_keyword_declaration(line, stripped, "function ", EntityKind::Function)
        .or_else(|| recover_keyword_declaration(line, stripped, "class ", EntityKind::Class))
        .or_else(|| {
            recover_keyword_declaration(line, stripped, "interface ", EntityKind::Interface)
        })
}

fn recover_go_declaration(line: &str, trimmed: &str) -> Option<(EntityKind, String, usize)> {
    if let Some(rest) = trimmed.strip_prefix("func ") {
        let after_receiver = if rest.trim_start().starts_with('(') {
            rest.find(')')
                .and_then(|idx| rest.get(idx + 1..))
                .unwrap_or(rest)
        } else {
            rest
        };
        let name = leading_identifier(after_receiver.trim_start())?;
        let column = line.find(&name)?;
        return Some((EntityKind::Function, name, column));
    }
    recover_keyword_declaration(line, trimmed, "type ", EntityKind::Class)
}

fn recover_c_like_function_declaration(
    line: &str,
    trimmed: &str,
) -> Option<(EntityKind, String, usize)> {
    if !trimmed.contains('(') || trimmed.contains("#define") {
        return None;
    }
    let before_paren = trimmed.split_once('(')?.0.trim_end();
    let declarator_tokens = before_paren
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|part| looks_like_identifier(part))
        .collect::<Vec<_>>();
    if declarator_tokens.len() < 2 {
        return None;
    }
    let name = before_paren
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .rev()
        .find(|part| looks_like_identifier(part))?
        .to_string();
    if !name.chars().any(|ch| ch.is_ascii_lowercase()) {
        return None;
    }
    let column = line.find(&name)?;
    Some((EntityKind::Function, name, column))
}

fn recover_keyword_declaration(
    line: &str,
    text: &str,
    keyword: &str,
    kind: EntityKind,
) -> Option<(EntityKind, String, usize)> {
    let rest = text.strip_prefix(keyword)?;
    let name = leading_identifier(rest.trim_start())?;
    let column = line.find(&name)?;
    Some((kind, name, column))
}

fn leading_identifier(text: &str) -> Option<String> {
    let name: String = text
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    if looks_like_identifier(&name) {
        Some(name)
    } else {
        None
    }
}

fn starts_with_comment_or_preprocessor(trimmed: &str) -> bool {
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('#')
        || trimmed.starts_with("--")
}

fn node_has_error_or_missing_descendant(node: Node<'_>) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    let has_error = node
        .children(&mut cursor)
        .any(node_has_error_or_missing_descendant);
    has_error
}

fn annotate_untrusted_syntax_entity(entity: &mut Entity, reason: &str) {
    entity.confidence = entity.confidence.min(0.45);
    entity
        .metadata
        .insert("claim_state".to_string(), "heuristic".into());
    entity.metadata.insert(
        "parser_reliability".to_string(),
        "untrusted_syntax_region".into(),
    );
    entity.metadata.insert(
        "unsupported_behavior_label".to_string(),
        "malformed_construct".into(),
    );
    entity
        .metadata
        .insert("parser_reliability_reason".to_string(), reason.into());
}

fn is_test_file_path(path: &str) -> bool {
    let normalized = normalize_repo_relative_path(path).to_ascii_lowercase();
    is_common_test_file_path(&normalized)
}

fn is_common_test_file_path(normalized_path: &str) -> bool {
    let file_name = normalized_path
        .rsplit('/')
        .next()
        .unwrap_or(normalized_path);
    path_has_component(
        normalized_path,
        &[
            "tests",
            "test",
            "__tests__",
            "spec",
            "specs",
            "examples",
            "benches",
            "fixtures",
            "fixture",
        ],
    ) || normalized_path.ends_with(".test.ts")
        || normalized_path.ends_with(".test.tsx")
        || normalized_path.ends_with(".test.js")
        || normalized_path.ends_with(".test.jsx")
        || normalized_path.ends_with(".spec.ts")
        || normalized_path.ends_with(".spec.tsx")
        || normalized_path.ends_with(".spec.js")
        || normalized_path.ends_with(".spec.jsx")
        || normalized_path.ends_with("_test.go")
        || normalized_path.ends_with("_test.py")
        || normalized_path.ends_with("_test.rb")
        || normalized_path.ends_with("_spec.rb")
        || normalized_path.ends_with("_test.php")
        || normalized_path.ends_with("_spec.php")
        || (file_name.starts_with("test_") && file_name.ends_with(".py"))
        || (file_name.starts_with("test_") && file_name.ends_with(".rb"))
        || (file_name.starts_with("test_") && file_name.ends_with(".php"))
        || file_name.ends_with("test.java")
        || file_name.ends_with("tests.java")
        || file_name.ends_with("spec.java")
        || file_name.ends_with("test.cs")
        || file_name.ends_with("tests.cs")
        || file_name.ends_with("spec.cs")
        || file_name.ends_with("test.php")
        || file_name.ends_with("testcase.php")
        || file_name.ends_with("test.rb")
        || file_name.ends_with("spec.rb")
}

fn is_common_mock_file_path(normalized_path: &str) -> bool {
    let file_name = normalized_path
        .rsplit('/')
        .next()
        .unwrap_or(normalized_path);
    path_has_component(
        normalized_path,
        &[
            "__mocks__",
            "mocks",
            "mock",
            "stubs",
            "stub",
            "fakes",
            "fake",
        ],
    ) || file_name.contains(".mock.")
        || file_name.contains(".stub.")
        || file_name.ends_with("_mock.py")
        || file_name.ends_with("_stub.py")
        || looks_like_mock_or_stub(file_name)
}

fn is_common_non_claimable_file_path(normalized_path: &str) -> bool {
    let file_name = normalized_path
        .rsplit('/')
        .next()
        .unwrap_or(normalized_path);
    path_has_component(
        normalized_path,
        &[
            "generated",
            "gen",
            "vendor",
            "vendored",
            "third_party",
            "node_modules",
        ],
    ) || file_name.ends_with(".d.ts")
        || file_name.contains(".generated.")
        || file_name.contains(".gen.")
}

fn path_has_component(normalized_path: &str, components: &[&str]) -> bool {
    normalized_path
        .split('/')
        .any(|part| components.iter().any(|component| part == *component))
}

#[derive(Debug, Clone)]
struct SourceRoleAnnotation {
    role: EvidenceRole,
    reason: &'static str,
    source: &'static str,
}

fn source_role_annotation(
    language: SourceLanguage,
    repo_relative_path: &str,
    kind: EntityKind,
    name: &str,
    qualified_name: &str,
    node: Option<Node<'_>>,
    source: &str,
) -> SourceRoleAnnotation {
    if matches!(kind, EntityKind::Mock | EntityKind::Stub) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Mock,
            reason: "entity kind is mock/stub",
            source: "entity_kind",
        };
    }
    if matches!(
        kind,
        EntityKind::TestFile
            | EntityKind::TestSuite
            | EntityKind::TestCase
            | EntityKind::Fixture
            | EntityKind::Assertion
    ) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Test,
            reason: "entity kind is test/assertion/fixture",
            source: "entity_kind",
        };
    }
    if language == SourceLanguage::Rust
        && node.is_some_and(|node| rust_node_has_test_attribute(node, source))
    {
        return SourceRoleAnnotation {
            role: EvidenceRole::Test,
            reason: "rust #[test] or #[cfg(test)] attribute",
            source: "rust_attribute",
        };
    }
    if qualified_name_has_tests_module(qualified_name) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Test,
            reason: "qualified name is inside a tests module",
            source: "module_path",
        };
    }
    let normalized_path = normalize_repo_relative_path(repo_relative_path).to_ascii_lowercase();
    if is_common_mock_file_path(&normalized_path) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Mock,
            reason: "file path is mock/stub evidence",
            source: "file_path",
        };
    }
    if is_test_file_path(repo_relative_path) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Test,
            reason: "file path is a test/spec path",
            source: "file_path",
        };
    }
    if is_common_non_claimable_file_path(&normalized_path) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Unknown,
            reason: "file path is generated/vendor/source-text-only evidence",
            source: "file_path",
        };
    }
    if looks_like_mock_or_stub(name) || looks_like_mock_or_stub(qualified_name) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Mock,
            reason: "name looks like mock/stub",
            source: "name",
        };
    }
    SourceRoleAnnotation {
        role: EvidenceRole::Production,
        reason: "default source role for non-test source",
        source: "extractor_default",
    }
}

fn edge_role_annotation(
    relation: RelationKind,
    span: &SourceSpan,
    head_role: EvidenceRole,
    tail_role: EvidenceRole,
) -> SourceRoleAnnotation {
    if matches!(relation, RelationKind::Mocks | RelationKind::Stubs) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Mock,
            reason: "relation kind is mock/stub evidence",
            source: "relation_kind",
        };
    }
    if matches!(
        relation,
        RelationKind::Tests
            | RelationKind::Asserts
            | RelationKind::Covers
            | RelationKind::FixturesFor
    ) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Test,
            reason: "relation kind is test/assertion evidence",
            source: "relation_kind",
        };
    }
    let normalized_path =
        normalize_repo_relative_path(&span.repo_relative_path).to_ascii_lowercase();
    if is_common_mock_file_path(&normalized_path) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Mock,
            reason: "edge source span is in mock/stub evidence",
            source: "file_path",
        };
    }
    if is_test_file_path(&span.repo_relative_path) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Test,
            reason: "edge source span is in a test/spec path",
            source: "file_path",
        };
    }
    if is_common_non_claimable_file_path(&normalized_path) {
        return SourceRoleAnnotation {
            role: EvidenceRole::Unknown,
            reason: "edge source span is generated/vendor/source-text-only evidence",
            source: "file_path",
        };
    }
    let role = combine_endpoint_roles(head_role, tail_role);
    let reason = match role {
        EvidenceRole::Production => "head and tail entities are production",
        EvidenceRole::Test => "head or tail entity is test evidence",
        EvidenceRole::Mock => "head or tail entity is mock/stub evidence",
        EvidenceRole::Mixed => "edge connects production and test/mock/unknown evidence",
        EvidenceRole::Unknown => "head or tail entity source role is unknown",
    };
    SourceRoleAnnotation {
        role,
        reason,
        source: "endpoint_source_role",
    }
}

fn combine_endpoint_roles(head_role: EvidenceRole, tail_role: EvidenceRole) -> EvidenceRole {
    if matches!(head_role, EvidenceRole::Mock) || matches!(tail_role, EvidenceRole::Mock) {
        return if head_role == tail_role {
            EvidenceRole::Mock
        } else {
            EvidenceRole::Mixed
        };
    }
    if matches!(head_role, EvidenceRole::Test) || matches!(tail_role, EvidenceRole::Test) {
        return EvidenceRole::Test;
    }
    if matches!(head_role, EvidenceRole::Unknown) || matches!(tail_role, EvidenceRole::Unknown) {
        return if matches!(head_role, EvidenceRole::Production)
            || matches!(tail_role, EvidenceRole::Production)
        {
            EvidenceRole::Mixed
        } else {
            EvidenceRole::Unknown
        };
    }
    EvidenceRole::Production
}

fn edge_context_for_role(role: EvidenceRole) -> EdgeContext {
    match role {
        EvidenceRole::Production => EdgeContext::Production,
        EvidenceRole::Test => EdgeContext::Test,
        EvidenceRole::Mock => EdgeContext::Mock,
        EvidenceRole::Mixed => EdgeContext::Mixed,
        EvidenceRole::Unknown => EdgeContext::Unknown,
    }
}

fn qualified_name_has_tests_module(value: &str) -> bool {
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    normalized == "tests"
        || normalized.starts_with("tests.")
        || normalized.contains(".tests.")
        || normalized.contains("::tests::")
        || normalized.ends_with(".tests")
        || normalized.ends_with("::tests")
}

fn looks_like_mock_or_stub(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("mock") || normalized.contains("stub")
}

fn is_test_case_declaration(
    language: SourceLanguage,
    repo_relative_path: &str,
    kind: EntityKind,
    name: &str,
    qualified_name: &str,
    node: Node<'_>,
    source: &str,
) -> bool {
    if !matches!(
        kind,
        EntityKind::Function | EntityKind::Method | EntityKind::Constructor
    ) {
        return false;
    }
    if language == SourceLanguage::Rust && rust_node_has_test_attribute(node, source) {
        return true;
    }
    let path_is_test = is_test_file_path(repo_relative_path);
    let lower = name.to_ascii_lowercase();
    match language {
        SourceLanguage::Python => path_is_test && lower.starts_with("test_"),
        SourceLanguage::Go => path_is_test && go_test_function_name(name),
        SourceLanguage::Java | SourceLanguage::CSharp => {
            path_is_test
                && (lower.starts_with("test")
                    || text_before_node(node, source, 4)
                        .to_ascii_lowercase()
                        .contains("@test")
                    || text_before_node(node, source, 4)
                        .to_ascii_lowercase()
                        .contains("[test"))
        }
        SourceLanguage::Ruby | SourceLanguage::Php => path_is_test && lower.starts_with("test"),
        SourceLanguage::Rust => {
            qualified_name_has_tests_module(qualified_name)
                && (lower.starts_with("test") || rust_node_has_test_attribute(node, source))
        }
        _ => false,
    }
}

fn go_test_function_name(name: &str) -> bool {
    name.strip_prefix("Test")
        .and_then(|tail| tail.chars().next())
        .is_some_and(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

fn text_before_node(node: Node<'_>, source: &str, max_lines: usize) -> String {
    let start_byte = node.start_byte().min(source.len());
    let bytes = &source.as_bytes()[..start_byte];
    let mut prefix_start = 0usize;
    let mut newline_count = 0usize;
    for index in (0..bytes.len()).rev() {
        if bytes[index] == b'\n' {
            newline_count += 1;
            if newline_count >= max_lines {
                prefix_start = index + 1;
                break;
            }
        }
    }
    String::from_utf8_lossy(&bytes[prefix_start..]).to_string()
}

fn is_proof_grade_exactness(exactness: Exactness) -> bool {
    matches!(
        exactness,
        Exactness::Exact
            | Exactness::CompilerVerified
            | Exactness::LspVerified
            | Exactness::ParserVerified
    )
}

fn is_generic_assert_call(language: SourceLanguage, lower_label: &str) -> bool {
    match language {
        SourceLanguage::Python => {
            lower_label == "assert"
                || lower_label.starts_with("self.assert")
                || lower_label.starts_with("pytest.")
        }
        SourceLanguage::Go => {
            lower_label.ends_with(".fatal")
                || lower_label.ends_with(".fatalf")
                || lower_label.ends_with(".error")
                || lower_label.ends_with(".errorf")
                || lower_label.contains("assert.")
                || lower_label.contains("require.")
        }
        SourceLanguage::Rust => lower_label.starts_with("assert"),
        SourceLanguage::Java | SourceLanguage::CSharp => {
            lower_label.contains("assert")
                || lower_label.ends_with("should")
                || lower_label.contains("assertthat")
        }
        SourceLanguage::Ruby | SourceLanguage::Php => {
            lower_label.contains("assert")
                || lower_label.contains("expect")
                || lower_label.contains("should")
        }
        _ => false,
    }
}

fn is_test_framework_callee_label(lower_label: &str) -> bool {
    is_test_case_call(lower_label)
        || is_assert_call(lower_label)
        || is_mock_call(lower_label)
        || is_stub_call(lower_label)
        || is_fixture_call(lower_label)
        || matches!(
            lower_label,
            "describe" | "it" | "test" | "beforeeach" | "aftereach" | "beforeall" | "afterall"
        )
        || lower_label.starts_with("vi.")
        || lower_label.starts_with("jest.")
}

fn is_generic_assertion_syntax_node(
    language: SourceLanguage,
    node: Node<'_>,
    source: &str,
) -> bool {
    match language {
        SourceLanguage::Python => node.kind() == "assert_statement",
        SourceLanguage::Rust => {
            node.kind() == "macro_invocation"
                && node_text(node, source)
                    .map(|text| text.trim_start().starts_with("assert"))
                    .unwrap_or(false)
        }
        _ => false,
    }
}

fn generic_mock_or_stub_relation(lower_label: &str) -> Option<RelationKind> {
    if contains_any(
        lower_label,
        &[
            "monkeypatch",
            "unittest.mock",
            "mock.patch",
            "magicmock",
            "gomock.",
            "mockgen",
            "mockito.",
            "moq.",
            "double",
            "createmock",
            "receive",
        ],
    ) || lower_label.contains("mock")
    {
        Some(RelationKind::Mocks)
    } else if lower_label.contains("stub") {
        Some(RelationKind::Stubs)
    } else {
        None
    }
}

fn first_assertion_target_argument<'tree>(
    language: SourceLanguage,
    arguments: Node<'tree>,
    source: &str,
) -> Option<Node<'tree>> {
    let mut cursor = arguments.walk();
    let mut children = arguments
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment");
    if language == SourceLanguage::Go {
        let first = children.next()?;
        if node_text(first, source).is_some_and(|text| text == "t") {
            return children.next();
        }
        return Some(first);
    }
    children.next()
}

fn rust_node_has_test_attribute(node: Node<'_>, source: &str) -> bool {
    let start_byte = node.start_byte().min(source.len());
    let bytes = &source.as_bytes()[..start_byte];
    let mut prefix_start = 0usize;
    let mut newline_count = 0usize;
    for index in (0..bytes.len()).rev() {
        if bytes[index] == b'\n' {
            newline_count += 1;
            if newline_count >= 5 {
                prefix_start = index + 1;
                break;
            }
        }
    }
    let prefix = String::from_utf8_lossy(&bytes[prefix_start..]);
    prefix.contains("#[test]") || prefix.contains("#[cfg(test)]")
}

fn looks_like_table_constant_name(name: &str) -> bool {
    let normalized = name.trim();
    normalized.len() > "table".len()
        && normalized.to_ascii_lowercase().ends_with("table")
        && normalized.chars().any(|ch| ch.is_ascii_alphabetic())
}

fn looks_like_persistence_writer_name(qualified_name: &str) -> bool {
    let name = qualified_name
        .rsplit(['.', ':', '/'])
        .find(|part| !part.is_empty())
        .unwrap_or(qualified_name)
        .to_ascii_lowercase();
    [
        "save", "insert", "update", "delete", "create", "write", "persist",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

fn is_scope_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Module
            | EntityKind::Class
            | EntityKind::Interface
            | EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
    )
}

fn is_static_executable_reference(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Function | EntityKind::Method | EntityKind::Constructor
    )
}

fn first_return_value(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    let mut children = node.named_children(&mut cursor);
    children.next()
}

fn call_callee_label(node: Node<'_>, source: &str) -> String {
    node.child_by_field_name("function")
        .map(|callee| expression_label(callee, source))
        .unwrap_or_else(|| "unknown_callee".to_string())
}

fn call_argument_nodes<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut cursor = arguments.walk();
    arguments
        .named_children(&mut cursor)
        .filter(|argument| argument.kind() != "comment")
        .collect()
}

fn expression_label(node: Node<'_>, source: &str) -> String {
    node_text(node, source)
        .map(|text| compact_extracted_label(&text, node))
        .unwrap_or_else(|| format!("{}@{}", node.kind(), node.start_byte()))
}

const DEFAULT_EXPRESSION_READ_IDENTIFIER_CAP: usize = 4096;
const LARGE_FILE_EXPRESSION_READ_IDENTIFIER_CAP: usize = 96;
const LARGE_FILE_READ_CAP_THRESHOLD_BYTES: usize = 256 * 1024;
const LARGE_EXPRESSION_READ_CAP_THRESHOLD_BYTES: usize = 1024;

fn expression_read_identifier_cap(
    repo_relative_path: &str,
    source_len: usize,
    node: Node<'_>,
) -> usize {
    let expression_bytes = node.end_byte().saturating_sub(node.start_byte());
    if source_len >= LARGE_FILE_READ_CAP_THRESHOLD_BYTES
        && expression_bytes >= LARGE_EXPRESSION_READ_CAP_THRESHOLD_BYTES
        && (is_test_file_path(repo_relative_path)
            || looks_generated_or_symbolic_fixture_path(repo_relative_path))
    {
        LARGE_FILE_EXPRESSION_READ_IDENTIFIER_CAP
    } else {
        DEFAULT_EXPRESSION_READ_IDENTIFIER_CAP
    }
}

fn looks_generated_or_symbolic_fixture_path(path: &str) -> bool {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    path.contains("/generated/")
        || path.contains("/rubi_tests/")
        || path.contains("/rubi/")
        || path.contains("/fixtures/")
}

fn collect_identifier_nodes<'a>(node: Node<'a>, identifiers: &mut Vec<Node<'a>>) {
    collect_identifier_nodes_capped(node, identifiers, usize::MAX);
}

fn collect_identifier_nodes_capped<'a>(
    node: Node<'a>,
    identifiers: &mut Vec<Node<'a>>,
    max_identifiers: usize,
) {
    if identifiers.len() >= max_identifiers {
        return;
    }
    if node.is_error() || node.is_missing() {
        return;
    }

    if node.kind() == "identifier" {
        if identifiers.len() < max_identifiers {
            identifiers.push(node);
        }
        return;
    }

    if matches!(
        node.kind(),
        "property_identifier"
            | "type_identifier"
            | "private_property_identifier"
            | "statement_identifier"
    ) {
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if identifiers.len() >= max_identifiers {
            break;
        }
        collect_identifier_nodes_capped(child, identifiers, max_identifiers);
    }
}

fn name_from_field_or_child(node: Node<'_>, source: &str) -> Option<String> {
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|child| node_text(child, source))
    {
        return Some(name);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier"
                | "property_identifier"
                | "private_property_identifier"
                | "type_identifier"
        ) {
            return node_text(child, source);
        }
    }

    None
}

fn variable_name(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|child| node_text(child, source))
        .or_else(|| {
            first_direct_named_text(
                node,
                source,
                &["identifier", "array_pattern", "object_pattern"],
            )
        })
}

fn variable_declarator_value_node(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("value").or_else(|| {
        let name = node.child_by_field_name("name")?;
        let mut cursor = node.walk();
        let value = node
            .named_children(&mut cursor)
            .find(|child| child.start_byte() >= name.end_byte() && child.id() != name.id());
        value
    })
}

fn parameter_name(node: Node<'_>, source: &str) -> Option<String> {
    if node.kind() == "identifier" {
        return node_text(node, source);
    }

    node.child_by_field_name("pattern")
        .and_then(|child| node_text(child, source))
        .or_else(|| {
            node.child_by_field_name("name")
                .and_then(|child| node_text(child, source))
        })
        .or_else(|| {
            first_direct_named_text(
                node,
                source,
                &["identifier", "object_pattern", "array_pattern"],
            )
        })
}

fn first_direct_named_text(node: Node<'_>, source: &str, kinds: &[&str]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if kinds.contains(&child.kind()) {
            return node_text(child, source);
        }
    }
    None
}

fn node_text(node: Node<'_>, source: &str) -> Option<String> {
    node.utf8_text(source.as_bytes())
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn statement_label(node: Node<'_>, source: &str) -> String {
    node_text(node, source)
        .map(|text| compact_extracted_label(&text, node))
        .unwrap_or_else(|| format!("{}@{}", node.kind(), node.start_byte()))
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn compact_identity_hash(value: &str) -> String {
    content_hash(value)
        .trim_start_matches("fnv64:")
        .chars()
        .take(MAX_IDENTITY_HASH_CHARS)
        .collect()
}

fn synthetic_source_identity(prefix: &str, node: Node<'_>, source_text: &str) -> String {
    format!(
        "{prefix}@{}-{}#{}",
        node.start_byte(),
        node.end_byte(),
        compact_identity_hash(source_text)
    )
}

fn compact_extracted_label(text: &str, node: Node<'_>) -> String {
    let collapsed = collapse_whitespace(text);
    if collapsed.chars().count() <= MAX_EXTRACTED_LABEL_CHARS {
        return collapsed;
    }
    let prefix = collapsed
        .chars()
        .take(MAX_EXTRACTED_LABEL_CHARS)
        .collect::<String>();
    format!("{prefix}...@{}", node.start_byte())
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn label_ends_with(label: &str, segment: &str) -> bool {
    label == segment || label.ends_with(&format!(".{segment}"))
}

fn route_method(label: &str) -> Option<&'static str> {
    let lower = label.to_ascii_lowercase();
    [
        "get", "post", "put", "patch", "delete", "options", "head", "all",
    ]
    .into_iter()
    .find(|method| label_ends_with(&lower, method))
}

fn is_direct_role_check_label(label: &str) -> bool {
    label == "checkrole"
        || label == "hasrole"
        || label == "requirerole"
        || label.ends_with(".checkrole")
        || label.ends_with(".hasrole")
        || label.ends_with(".requirerole")
}

fn string_argument(arguments: &[Node<'_>], index: usize, source: &str) -> Option<String> {
    arguments
        .get(index)
        .and_then(|argument| string_literal_value(*argument, source))
}

fn first_string_argument(arguments: &[Node<'_>], source: &str) -> Option<String> {
    arguments
        .iter()
        .find_map(|argument| string_literal_value(*argument, source))
}

fn string_literal_value(node: Node<'_>, source: &str) -> Option<String> {
    let text = node_text(node, source)?;
    strip_string_literal(&text)
}

fn strip_string_literal(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let first = trimmed.chars().next()?;
    let last = trimmed.chars().last()?;
    if matches!(first, '"' | '\'' | '`') && first == last && trimmed.len() >= 2 {
        Some(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        None
    }
}

fn contains_taint_source(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "req.body",
            "req.query",
            "req.params",
            "request.body",
            "request.query",
            "process.env",
            "cookies",
            "headers",
        ],
    )
}

fn table_column_method(label: &str) -> bool {
    [
        "string",
        "text",
        "integer",
        "increments",
        "boolean",
        "timestamp",
        "datetime",
        "uuid",
        "json",
        "decimal",
    ]
    .into_iter()
    .any(|method| label_ends_with(label, method))
}

fn sql_table_relation(sql: &str) -> Option<(RelationKind, String)> {
    let lower = sql.to_ascii_lowercase();
    let normalized = normalize_sql_tokens(&lower)
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        return None;
    }
    if normalized.first().is_some_and(|token| token == "select") {
        return token_after(&normalized, "from").map(|table| (RelationKind::ReadsTable, table));
    }
    if normalized.first().is_some_and(|token| token == "insert") {
        return token_after(&normalized, "into").map(|table| (RelationKind::WritesTable, table));
    }
    if normalized.first().is_some_and(|token| token == "update") {
        return normalized
            .get(1)
            .cloned()
            .map(|table| (RelationKind::WritesTable, table));
    }
    if normalized.first().is_some_and(|token| token == "delete") {
        return token_after(&normalized, "from").map(|table| (RelationKind::WritesTable, table));
    }
    if normalized.first().is_some_and(|token| token == "alter") {
        return token_after(&normalized, "table")
            .map(|table| (RelationKind::DependsOnSchema, table));
    }
    None
}

fn sql_altered_column(sql: &str) -> Option<String> {
    let lower = sql.to_ascii_lowercase();
    let tokens = normalize_sql_tokens(&lower)
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    token_after(&tokens, "column")
}

fn normalize_sql_tokens(sql: &str) -> String {
    sql.chars()
        .map(|ch| {
            if matches!(ch, '(' | ')' | ',' | ';' | '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

fn token_after(tokens: &[String], marker: &str) -> Option<String> {
    tokens
        .iter()
        .position(|token| token == marker)
        .and_then(|index| tokens.get(index + 1))
        .cloned()
}

fn is_test_case_call(label: &str) -> bool {
    matches!(label, "it" | "test") || label.ends_with(".it") || label.ends_with(".test")
}

fn is_assert_call(label: &str) -> bool {
    label == "expect" || label.starts_with("expect.") || label.starts_with("assert")
}

fn is_mock_call(label: &str) -> bool {
    contains_any(
        label,
        &["vi.mock", "jest.mock", "spyon", "vi.fn", "jest.fn", ".mock"],
    ) || matches!(label, "mock")
}

fn is_stub_call(label: &str) -> bool {
    contains_any(label, &["stub", "stubenv"])
}

fn is_fixture_call(label: &str) -> bool {
    matches!(label, "beforeeach" | "beforeall" | "aftereach" | "afterall")
        || contains_any(label, &["fixture", "setup"])
}

fn collect_call_expression_nodes<'a>(node: Node<'a>, calls: &mut Vec<Node<'a>>) {
    if node.kind() == "call_expression" {
        calls.push(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_call_expression_nodes(child, calls);
    }
}

fn is_generic_call_node(language: SourceLanguage, node: Node<'_>) -> bool {
    match language {
        SourceLanguage::Python => node.kind() == "call",
        SourceLanguage::Go => node.kind() == "call_expression",
        SourceLanguage::Rust => matches!(node.kind(), "call_expression" | "method_call_expression"),
        SourceLanguage::C | SourceLanguage::Cpp => node.kind() == "call_expression",
        _ => false,
    }
}

fn generic_call_callee_node(language: SourceLanguage, node: Node<'_>) -> Option<Node<'_>> {
    match language {
        SourceLanguage::Python => node
            .child_by_field_name("function")
            .or_else(|| first_named_child(node)),
        SourceLanguage::Go | SourceLanguage::C | SourceLanguage::Cpp => node
            .child_by_field_name("function")
            .or_else(|| first_named_child(node)),
        SourceLanguage::Rust if node.kind() == "method_call_expression" => node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("field"))
            .or_else(|| first_named_child(node)),
        SourceLanguage::Rust => node
            .child_by_field_name("function")
            .or_else(|| first_named_child(node)),
        _ => None,
    }
}

fn generic_call_arguments_node(language: SourceLanguage, node: Node<'_>) -> Option<Node<'_>> {
    match language {
        SourceLanguage::Python | SourceLanguage::Go | SourceLanguage::C | SourceLanguage::Cpp => {
            node.child_by_field_name("arguments")
        }
        SourceLanguage::Rust => node
            .child_by_field_name("arguments")
            .or_else(|| child_by_kind(node, "arguments")),
        _ => None,
    }
}

fn go_statement_call_node(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("call").or_else(|| {
        let mut cursor = node.walk();
        let call = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "call_expression");
        call
    })
}

fn is_generic_assignment_node(language: SourceLanguage, node: Node<'_>) -> bool {
    match language {
        SourceLanguage::Python => matches!(node.kind(), "assignment" | "augmented_assignment"),
        SourceLanguage::Go => matches!(
            node.kind(),
            "short_var_declaration" | "var_spec" | "assignment_statement"
        ),
        SourceLanguage::Rust => matches!(node.kind(), "let_declaration" | "assignment_expression"),
        SourceLanguage::C | SourceLanguage::Cpp => {
            matches!(node.kind(), "init_declarator" | "assignment_expression")
        }
        _ => false,
    }
}

fn generic_assignment_target_node(language: SourceLanguage, node: Node<'_>) -> Option<Node<'_>> {
    match language {
        SourceLanguage::Rust if node.kind() == "let_declaration" => {
            node.child_by_field_name("pattern")
        }
        SourceLanguage::C | SourceLanguage::Cpp if node.kind() == "init_declarator" => {
            node.child_by_field_name("declarator")
        }
        _ => node
            .child_by_field_name("left")
            .or_else(|| node.child_by_field_name("name"))
            .or_else(|| node.child_by_field_name("declarator"))
            .or_else(|| first_named_child(node)),
    }
}

fn generic_assignment_value_node(language: SourceLanguage, node: Node<'_>) -> Option<Node<'_>> {
    match language {
        SourceLanguage::Rust if node.kind() == "let_declaration" => {
            node.child_by_field_name("value")
        }
        SourceLanguage::C | SourceLanguage::Cpp if node.kind() == "init_declarator" => {
            node.child_by_field_name("value")
        }
        _ => node
            .child_by_field_name("right")
            .or_else(|| node.child_by_field_name("value")),
    }
}

fn generic_assignment_declares_local(language: SourceLanguage, node: Node<'_>) -> bool {
    match language {
        SourceLanguage::Python => node.kind() == "assignment",
        SourceLanguage::Go => matches!(node.kind(), "short_var_declaration" | "var_spec"),
        SourceLanguage::Rust => node.kind() == "let_declaration",
        SourceLanguage::C | SourceLanguage::Cpp => node.kind() == "init_declarator",
        _ => false,
    }
}

fn generic_assignment_target_is_identifier(node: Node<'_>) -> bool {
    generic_identifier_kind(node.kind())
        || matches!(node.kind(), "identifier_pattern" | "scoped_identifier")
}

fn generic_assignment_single_identifier_name(node: Node<'_>, source: &str) -> Option<String> {
    if generic_assignment_target_is_identifier(node) {
        return deepest_identifier(node, source);
    }
    let mut identifiers = Vec::new();
    collect_identifier_nodes(node, &mut identifiers);
    if identifiers.len() == 1 {
        return node_text(identifiers[0], source)
            .map(clean_decl_name)
            .filter(|name| looks_like_identifier(name));
    }
    None
}

fn generic_assignment_target_is_property(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "member_expression"
            | "field_expression"
            | "subscript_expression"
            | "index_expression"
            | "selector_expression"
            | "attribute"
    )
}

fn is_generic_return_node(language: SourceLanguage, node: Node<'_>) -> bool {
    matches!(
        (language, node.kind()),
        (SourceLanguage::Python, "return_statement")
            | (SourceLanguage::Go, "return_statement")
            | (SourceLanguage::Rust, "return_expression")
            | (SourceLanguage::C, "return_statement")
            | (SourceLanguage::Cpp, "return_statement")
    )
}

fn generic_return_value_node(language: SourceLanguage, node: Node<'_>) -> Option<Node<'_>> {
    match language {
        SourceLanguage::Python | SourceLanguage::Go | SourceLanguage::C | SourceLanguage::Cpp => {
            node.child_by_field_name("argument")
                .or_else(|| first_named_child(node))
        }
        SourceLanguage::Rust => node
            .child_by_field_name("value")
            .or_else(|| first_named_child(node)),
        _ => None,
    }
}

fn generic_callee_symbol_name(label: &str) -> Option<String> {
    label
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'))
        .rfind(|part| !part.is_empty())
        .map(|part| part.trim_matches(':').to_string())
        .filter(|part| looks_like_identifier(part))
}

fn first_named_child<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let child = node.named_children(&mut cursor).next();
    child
}

fn child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let child = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    child
}

fn generic_decl_kind(language: SourceLanguage, node: Node<'_>) -> Option<EntityKind> {
    let kind = node.kind();
    match kind {
        "class_declaration" | "class_definition" | "class" | "class_specifier" => {
            Some(EntityKind::Class)
        }
        "interface_declaration" | "interface_type" => Some(EntityKind::Interface),
        "trait_item" => Some(EntityKind::Trait),
        "enum_item" | "enum_declaration" | "enum_specifier" => Some(EntityKind::Enum),
        "struct_item" | "struct_type" | "struct_specifier" => Some(EntityKind::Class),
        "function_definition" | "function_declaration" | "function_item" => {
            Some(EntityKind::Function)
        }
        "method_definition" | "method_declaration" | "method_item" | "method" => {
            Some(EntityKind::Method)
        }
        "constructor_declaration" => Some(EntityKind::Constructor),
        "type_spec" if has_descendant_kind(node, "struct_type") => Some(EntityKind::Class),
        "type_spec" if has_descendant_kind(node, "interface_type") => Some(EntityKind::Interface),
        "module" if language == SourceLanguage::Ruby => Some(EntityKind::Module),
        "namespace_definition" | "namespace_declaration" => Some(EntityKind::Module),
        "mod_item" => Some(EntityKind::Module),
        "type_declaration" | "type_alias" | "type_item" => Some(EntityKind::Type),
        _ => None,
    }
}

fn rust_function_decl_within_impl_item(node: Node<'_>) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "impl_item" {
            return true;
        }
        if generic_decl_kind(SourceLanguage::Rust, parent).is_some() {
            return false;
        }
        current = parent.parent();
    }
    false
}

fn rust_inherent_impl_type_name(node: Node<'_>, source: &str) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "impl_item" {
            let text = node_text(parent, source)?;
            let header = text.split('{').next().unwrap_or(text.as_str()).trim();
            let after_impl = header.strip_prefix("impl")?.trim();
            let after_generics = strip_leading_rust_generic_params(after_impl).trim();
            if after_generics.contains(" for ") {
                return None;
            }
            return rust_type_name_from_text(after_generics);
        }
        current = parent.parent();
    }
    None
}

fn strip_leading_rust_generic_params(text: &str) -> &str {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('<') {
        return trimmed;
    }
    let mut depth = 0i32;
    for (index, ch) in trimmed.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return trimmed[index + ch.len_utf8()..].trim_start();
                }
            }
            _ => {}
        }
    }
    trimmed
}

fn rust_parameter_type_name(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("type")
        .and_then(|child| node_text(child, source))
        .and_then(|text| rust_type_name_from_text(&text))
        .or_else(|| {
            let text = node_text(node, source)?;
            let (_, after_colon) = text.split_once(':')?;
            let before_default = after_colon
                .split(['=', ','])
                .next()
                .unwrap_or(after_colon)
                .trim();
            rust_type_name_from_text(before_default)
        })
}

fn rust_local_type_name_from_assignment(
    assignment: Node<'_>,
    local_name: &str,
    source: &str,
) -> Option<String> {
    let text = node_text(assignment, source)?;
    let after_let = text.strip_prefix("let")?.trim_start();
    let after_mut = after_let
        .strip_prefix("mut ")
        .unwrap_or(after_let)
        .trim_start();
    let rest = after_mut.strip_prefix(local_name)?.trim_start();
    if let Some(after_colon) = rest.strip_prefix(':') {
        let before_value = after_colon
            .split(['=', ';'])
            .next()
            .unwrap_or(after_colon)
            .trim();
        if let Some(type_name) = rust_type_name_from_text(before_value) {
            return Some(type_name);
        }
    }
    let after_equals = rest
        .split_once('=')
        .map(|(_, value)| value)
        .or_else(|| text.split_once('=').map(|(_, value)| value))?
        .trim()
        .trim_end_matches(';')
        .trim();
    rust_type_name_from_constructor_expr(after_equals)
}

fn rust_type_name_from_constructor_expr(expr: &str) -> Option<String> {
    let before_paren = expr.split('(').next().unwrap_or(expr).trim();
    let before_brace = before_paren
        .split('{')
        .next()
        .unwrap_or(before_paren)
        .trim();
    let parts = rust_path_parts(before_brace);
    if parts.is_empty() {
        return None;
    }
    if parts.len() >= 2 {
        let candidate = &parts[parts.len() - 2];
        if looks_like_type_identifier(candidate) {
            return Some(candidate.clone());
        }
    }
    let candidate = parts.last()?;
    if looks_like_type_identifier(candidate) {
        return Some(candidate.clone());
    }
    None
}

fn rust_method_call_label(
    call_node: Node<'_>,
    callee_node: Node<'_>,
    source: &str,
) -> Option<String> {
    let receiver = call_node
        .child_by_field_name("receiver")
        .and_then(|node| node_text(node, source))
        .map(|text| compact_extracted_label(&text, callee_node))?;
    let method = node_text(callee_node, source).map(clean_decl_name)?;
    if receiver.is_empty() || method.is_empty() {
        return None;
    }
    Some(format!("{receiver}.{method}"))
}

fn rust_method_label_parts(label: &str) -> Option<(&str, &str)> {
    if label.contains("::") {
        return None;
    }
    let (receiver, method) = label.rsplit_once('.')?;
    if !looks_like_identifier(receiver) || !looks_like_identifier(method) {
        return None;
    }
    Some((receiver, method))
}

fn rust_path_parts(raw_path: &str) -> Vec<String> {
    let base = raw_path
        .split('(')
        .next()
        .unwrap_or(raw_path)
        .trim()
        .trim_end_matches(';');
    base.split("::")
        .filter_map(|part| {
            let cleaned = part
                .split(['<', '{', ' ', '\t', '\r', '\n'])
                .next()
                .unwrap_or(part)
                .trim()
                .trim_matches(':');
            if looks_like_identifier(cleaned)
                || matches!(cleaned, "crate" | "self" | "super" | "Self")
            {
                Some(cleaned.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn rust_type_name_from_text(text: &str) -> Option<String> {
    let without_reference = text
        .trim()
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim();
    let parts = rust_path_parts(without_reference);
    parts
        .last()
        .filter(|part| looks_like_type_identifier(part))
        .cloned()
}

fn looks_like_type_identifier(value: &str) -> bool {
    looks_like_identifier(value)
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_uppercase())
}

fn generic_decl_name(node: Node<'_>, source: &str) -> Option<String> {
    name_from_field_or_child(node, source)
        .or_else(|| {
            node.child_by_field_name("declarator")
                .and_then(|child| deepest_identifier(child, source))
        })
        .or_else(|| deepest_identifier(node, source))
        .map(clean_decl_name)
        .filter(|name| looks_like_identifier(name))
}

fn deepest_identifier(node: Node<'_>, source: &str) -> Option<String> {
    if node.is_error() || node.is_missing() {
        return None;
    }
    if generic_identifier_kind(node.kind()) {
        return node_text(node, source);
    }
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|child| node_text(child, source))
    {
        return Some(name);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(name) = deepest_identifier(child, source) {
            return Some(name);
        }
    }
    None
}

fn generic_identifier_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "field_identifier"
            | "type_identifier"
            | "property_identifier"
            | "constant"
            | "scope_identifier"
    )
}

fn has_descendant_kind(node: Node<'_>, wanted: &str) -> bool {
    if node.kind() == wanted {
        return true;
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .any(|child| has_descendant_kind(child, wanted));
    found
}

fn generic_parameter_container(kind: &str) -> bool {
    matches!(
        kind,
        "parameters"
            | "parameter_list"
            | "formal_parameters"
            | "parameter_declaration"
            | "parameter_declarations"
    )
}

fn generic_parameter_node(kind: &str) -> bool {
    kind.contains("parameter")
        || matches!(
            kind,
            "identifier" | "typed_parameter" | "default_parameter" | "optional_parameter"
        )
}

fn generic_parameter_name(node: Node<'_>, source: &str) -> Option<String> {
    parameter_name(node, source)
        .or_else(|| deepest_identifier(node, source))
        .map(clean_decl_name)
        .filter(|name| looks_like_identifier(name))
}

fn generic_local_variable_name(
    language: SourceLanguage,
    node: Node<'_>,
    source: &str,
) -> Option<String> {
    let kind = node.kind();
    let candidate = match (language, kind) {
        (SourceLanguage::Python, "assignment") => node.child_by_field_name("left"),
        (SourceLanguage::Go, "short_var_declaration") | (SourceLanguage::Go, "var_declaration") => {
            node.child_by_field_name("left")
        }
        (SourceLanguage::Rust, "let_declaration") => node.child_by_field_name("pattern"),
        (_, "variable_declarator") | (_, "init_declarator") => {
            node.child_by_field_name("declarator")
        }
        _ => None,
    };
    candidate
        .and_then(|child| deepest_identifier(child, source))
        .map(clean_decl_name)
        .filter(|name| looks_like_identifier(name))
}

fn is_generic_import_node(language: SourceLanguage, node: Node<'_>) -> bool {
    let kind = node.kind();
    matches!(
        kind,
        "import_statement"
            | "import_from_statement"
            | "import_declaration"
            | "import_spec"
            | "use_declaration"
            | "preproc_include"
            | "include_expression"
            | "using_directive"
            | "using_statement"
    ) || (language == SourceLanguage::Php && kind == "namespace_use_declaration")
}

fn generic_import_name(language: SourceLanguage, node: Node<'_>, source: &str) -> Option<String> {
    match language {
        SourceLanguage::Python => node
            .child_by_field_name("name")
            .and_then(|child| node_text(child, source))
            .or_else(|| node_text(node, source)),
        SourceLanguage::Go => {
            string_literal_value(node, source).or_else(|| node_text(node, source))
        }
        SourceLanguage::Rust => node_text(node, source),
        SourceLanguage::C | SourceLanguage::Cpp => node_text(node, source),
        _ => node_text(node, source),
    }
    .map(|value| compact_extracted_label(&value, node))
}

/// The unresolved-import lane target label for a parsed import binding
/// (MVP3.9.5.4). Only languages whose import syntax names a concrete target
/// path participate in v1: Rust `use` paths, Python module.name pairs, and Go
/// import specifiers. JS/TS module specifiers are mostly relative paths that
/// classify dynamic downstream and are skipped to avoid lane noise.
fn unresolved_import_target_label(
    language: SourceLanguage,
    binding: &ImportBinding,
) -> Option<String> {
    let label = match language {
        SourceLanguage::Rust => binding.imported_name.clone()?,
        SourceLanguage::Python => match (&binding.module_specifier, &binding.imported_name) {
            (Some(module), Some(imported)) if !module.is_empty() && module != imported => {
                format!("{module}.{imported}")
            }
            (_, Some(imported)) => imported.clone(),
            (Some(module), None) => module.clone(),
            _ => return None,
        },
        SourceLanguage::Go => binding.module_specifier.clone()?,
        _ => return None,
    };
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    Some(label.to_string())
}

fn statement_import_binding(language: SourceLanguage, name: &str) -> ImportBinding {
    match language {
        SourceLanguage::C | SourceLanguage::Cpp => ImportBinding::preprocessor_include(
            name.to_string(),
            include_specifier(name),
            if language == SourceLanguage::C {
                "c_include"
            } else {
                "cpp_include"
            },
        ),
        _ => ImportBinding::parser_observed(name.to_string(), None, None, "statement"),
    }
}

fn import_bindings_for_node(
    language: SourceLanguage,
    node: Node<'_>,
    source: &str,
) -> Vec<ImportBinding> {
    let Some(text) = node_text(node, source) else {
        return Vec::new();
    };
    match language {
        SourceLanguage::JavaScript
        | SourceLanguage::Jsx
        | SourceLanguage::TypeScript
        | SourceLanguage::Tsx => parse_js_import_bindings(&text),
        SourceLanguage::Python => parse_python_import_bindings(&text),
        SourceLanguage::Rust => parse_rust_use_bindings(&text),
        SourceLanguage::Go => parse_go_import_bindings(&text),
        SourceLanguage::C => parse_c_include_binding(&text, "c_include"),
        SourceLanguage::Cpp => parse_c_include_binding(&text, "cpp_include"),
        SourceLanguage::Java => parse_java_import_bindings(&text),
        SourceLanguage::CSharp => parse_csharp_using_bindings(&text),
        SourceLanguage::Ruby => parse_ruby_import_bindings(&text),
        SourceLanguage::Php => parse_php_import_bindings(&text),
    }
}

fn reexport_bindings_for_node(
    language: SourceLanguage,
    node: Node<'_>,
    source: &str,
) -> Vec<ImportBinding> {
    if !language.is_javascript_family() {
        return Vec::new();
    }
    node_text(node, source)
        .map(|text| parse_js_reexport_bindings(&text))
        .unwrap_or_default()
}

fn annotate_import_artifact(
    entities: &mut [Entity],
    edges: &mut [Edge],
    import_id: &str,
    binding: &ImportBinding,
) {
    if let Some(entity) = entities.iter_mut().find(|entity| entity.id == import_id) {
        entity.metadata.insert("tier".to_string(), "1".into());
        entity.metadata.insert(
            "import_kind".to_string(),
            binding.import_kind.clone().into(),
        );
        entity
            .metadata
            .insert("local_name".to_string(), binding.local_name.clone().into());
        if let Some(imported_name) = &binding.imported_name {
            entity
                .metadata
                .insert("imported_name".to_string(), imported_name.clone().into());
        }
        if let Some(module_specifier) = &binding.module_specifier {
            entity.metadata.insert(
                "module_specifier".to_string(),
                module_specifier.clone().into(),
            );
        }
        entity.metadata.insert(
            "claim_state".to_string(),
            binding.claim_state.clone().into(),
        );
        entity.metadata.insert(
            "syntax_claim_state".to_string(),
            binding.syntax_claim_state.clone().into(),
        );
        entity.metadata.insert(
            "target_resolution_claim_state".to_string(),
            binding.target_resolution_claim_state.clone().into(),
        );
        entity
            .metadata
            .insert("resolution".to_string(), binding.resolution.clone().into());
        if let Some(reason) = &binding.unsupported_reason {
            entity
                .metadata
                .insert("unsupported_reason".to_string(), reason.clone().into());
        }
    }
    for edge in edges.iter_mut().filter(|edge| {
        edge.tail_id == import_id
            && matches!(
                edge.relation,
                RelationKind::Imports | RelationKind::Contains | RelationKind::DefinedIn
            )
    }) {
        edge.metadata.insert("tier".to_string(), "1".into());
        edge.metadata.insert(
            "claim_state".to_string(),
            binding.claim_state.clone().into(),
        );
        edge.metadata.insert(
            "syntax_claim_state".to_string(),
            binding.syntax_claim_state.clone().into(),
        );
        edge.metadata.insert(
            "target_resolution_claim_state".to_string(),
            binding.target_resolution_claim_state.clone().into(),
        );
        edge.metadata
            .insert("resolution".to_string(), binding.resolution.clone().into());
    }
}

fn annotate_export_metadata(entity: &mut Entity, language: SourceLanguage, text: &str) {
    let export_kind = classify_export_kind(language, text);
    entity.metadata.insert("tier".to_string(), "1".into());
    entity
        .metadata
        .insert("export_kind".to_string(), export_kind.into());
    entity
        .metadata
        .insert("syntax_claim_state".to_string(), "exact".into());
    if export_kind == "wildcard_reexport" {
        entity
            .metadata
            .insert("claim_state".to_string(), "partial".into());
        entity.metadata.insert(
            "target_resolution_claim_state".to_string(),
            "unsupported".into(),
        );
        entity.metadata.insert(
            "unsupported_reason".to_string(),
            "wildcard re-export target set is not expanded by parser-only extraction".into(),
        );
    }
}

fn annotate_local_export_metadata(entity: &mut Entity, local_name: &str) {
    entity.metadata.insert("tier".to_string(), "1".into());
    entity
        .metadata
        .insert("export_kind".to_string(), "named_export".into());
    entity
        .metadata
        .insert("local_name".to_string(), local_name.to_string().into());
    entity
        .metadata
        .insert("syntax_claim_state".to_string(), "exact".into());
    entity
        .metadata
        .insert("claim_state".to_string(), "partial".into());
    entity.metadata.insert(
        "target_resolution_claim_state".to_string(),
        "unresolved".into(),
    );
    entity.metadata.insert(
        "resolution".to_string(),
        "parser_observed_local_export_unresolved_declaration".into(),
    );
}

fn classify_export_kind(language: SourceLanguage, text: &str) -> &'static str {
    let trimmed = trim_statement(text);
    if language.is_javascript_family() {
        if trimmed.starts_with("export default") {
            return "default";
        }
        if trimmed.starts_with("export *") && trimmed.contains(" from ") {
            return "wildcard_reexport";
        }
        if trimmed.starts_with("export {") && trimmed.contains(" from ") {
            return "named_reexport";
        }
        if trimmed.starts_with("export {") {
            return "named";
        }
        if trimmed.starts_with("export ") {
            return "declaration";
        }
    }
    if language == SourceLanguage::Rust && trimmed.starts_with("pub ") {
        return "public_declaration";
    }
    if matches!(language, SourceLanguage::Java | SourceLanguage::CSharp)
        && trimmed.starts_with("public ")
    {
        return "public_declaration";
    }
    if language == SourceLanguage::Go {
        return "exported_identifier";
    }
    "unknown"
}

fn parse_js_import_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text);
    let Some(after_import) = trimmed.strip_prefix("import ") else {
        return Vec::new();
    };
    if after_import.starts_with('(') {
        return Vec::new();
    }
    let (clause, module_specifier) =
        if let Some((clause, after_from)) = after_import.split_once(" from ") {
            (clause.trim(), first_quoted_literal(after_from))
        } else {
            let module = first_quoted_literal(after_import);
            if let Some(module_specifier) = module {
                return vec![ImportBinding::parser_observed(
                    module_specifier.clone(),
                    None,
                    Some(module_specifier),
                    "side_effect",
                )];
            }
            return Vec::new();
        };
    let Some(module_specifier) = module_specifier else {
        return Vec::new();
    };
    let mut bindings = Vec::new();
    let clause = strip_type_prefix(clause);
    if clause.starts_with('{') {
        bindings.extend(parse_js_named_import_list(
            clause,
            &module_specifier,
            "named",
        ));
        return bindings;
    }
    if let Some(namespace_name) = parse_js_namespace_import(clause) {
        bindings.push(ImportBinding::parser_observed(
            namespace_name,
            Some("*".to_string()),
            Some(module_specifier),
            "namespace",
        ));
        return bindings;
    }
    if let Some((default_part, rest)) = clause.split_once(',') {
        let default_name = strip_type_prefix(default_part.trim());
        if looks_like_identifier(default_name) {
            bindings.push(ImportBinding::parser_observed(
                default_name.to_string(),
                Some("default".to_string()),
                Some(module_specifier.clone()),
                "default",
            ));
        }
        let rest = rest.trim();
        if rest.starts_with('{') {
            bindings.extend(parse_js_named_import_list(rest, &module_specifier, "named"));
        } else if let Some(namespace_name) = parse_js_namespace_import(rest) {
            bindings.push(ImportBinding::parser_observed(
                namespace_name,
                Some("*".to_string()),
                Some(module_specifier),
                "namespace",
            ));
        }
        return bindings;
    }
    if looks_like_identifier(clause) {
        bindings.push(ImportBinding::parser_observed(
            clause.to_string(),
            Some("default".to_string()),
            Some(module_specifier),
            "default",
        ));
    }
    bindings
}

fn parse_js_reexport_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text);
    if !trimmed.starts_with("export ") || !trimmed.contains(" from ") {
        return Vec::new();
    }
    let Some((export_clause, after_from)) = trimmed["export ".len()..].split_once(" from ") else {
        return Vec::new();
    };
    let Some(module_specifier) = first_quoted_literal(after_from) else {
        return Vec::new();
    };
    let export_clause = export_clause.trim();
    if export_clause == "*" {
        return vec![ImportBinding::target_unsupported(
            "*",
            Some("*".to_string()),
            Some(module_specifier),
            "wildcard_reexport",
            "wildcard re-export target set is not expanded by parser-only extraction",
        )];
    }
    parse_js_named_import_list(export_clause, &module_specifier, "named_reexport")
}

fn local_export_bindings_for_node(
    language: SourceLanguage,
    node: Node<'_>,
    source: &str,
) -> Vec<(String, String)> {
    if !language.is_javascript_family() {
        return Vec::new();
    }
    node_text(node, source)
        .map(|text| parse_js_local_export_bindings(&text))
        .unwrap_or_default()
}

fn parse_js_local_export_bindings(text: &str) -> Vec<(String, String)> {
    let trimmed = trim_statement(text);
    if !trimmed.starts_with("export {") || trimmed.contains(" from ") {
        return Vec::new();
    }
    let Some(open) = trimmed.find('{') else {
        return Vec::new();
    };
    let Some(close) = trimmed[open + 1..].find('}') else {
        return Vec::new();
    };
    let body = &trimmed[open + 1..open + 1 + close];
    body.split(',')
        .filter_map(|item| {
            let item = strip_type_prefix(item.trim());
            if item.is_empty() {
                return None;
            }
            let (local, exported) = split_alias(item).unwrap_or((item, item));
            if looks_like_identifier(local) && looks_like_identifier(exported) {
                Some((local.to_string(), exported.to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn parse_js_named_import_list(
    clause: &str,
    module_specifier: &str,
    import_kind: &str,
) -> Vec<ImportBinding> {
    let inner = clause
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    inner
        .split(',')
        .filter_map(|raw| {
            let item = strip_type_prefix(raw.trim());
            if item.is_empty() {
                return None;
            }
            let (imported, local) = split_alias(item).unwrap_or((item, item));
            if !looks_like_identifier(imported) || !looks_like_identifier(local) {
                return None;
            }
            Some(ImportBinding::parser_observed(
                local.to_string(),
                Some(imported.to_string()),
                Some(module_specifier.to_string()),
                import_kind.to_string(),
            ))
        })
        .collect()
}

fn parse_js_namespace_import(clause: &str) -> Option<String> {
    clause
        .trim()
        .strip_prefix("* as ")
        .map(str::trim)
        .filter(|name| looks_like_identifier(name))
        .map(str::to_string)
}

fn parse_python_import_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text);
    if let Some(after_import) = trimmed.strip_prefix("import ") {
        return after_import
            .split(',')
            .filter_map(|item| {
                let item = item.trim();
                if item.is_empty() {
                    return None;
                }
                let (imported, local) = split_alias(item).unwrap_or_else(|| {
                    let local = item.rsplit('.').next().unwrap_or(item);
                    (item, local)
                });
                Some(ImportBinding::target_unsupported(
                    local.to_string(),
                    Some(imported.to_string()),
                    Some(imported.to_string()),
                    "python_import",
                    "python import target resolution requires module search path execution context",
                ))
            })
            .collect();
    }
    let Some(after_from) = trimmed.strip_prefix("from ") else {
        return Vec::new();
    };
    let Some((module_specifier, imported_list)) = after_from.split_once(" import ") else {
        return Vec::new();
    };
    imported_list
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return None;
            }
            if item == "*" {
                return Some(ImportBinding::target_unsupported(
                    "*",
                    Some("*".to_string()),
                    Some(module_specifier.trim().to_string()),
                    "python_from_wildcard",
                    "wildcard import target set is not expanded by parser-only extraction",
                ));
            }
            let (imported, local) = split_alias(item).unwrap_or((item, item));
            if !looks_like_identifier(local) {
                return None;
            }
            Some(ImportBinding::target_unsupported(
                local.to_string(),
                Some(imported.to_string()),
                Some(module_specifier.trim().to_string()),
                "python_from_import",
                "python import target resolution requires module search path execution context",
            ))
        })
        .collect()
}

fn parse_rust_use_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text).trim_start_matches("pub ").trim();
    let Some(path) = trimmed.strip_prefix("use ") else {
        return Vec::new();
    };
    let path = path.trim();
    if let Some(open) = path.find('{') {
        let base = path[..open].trim_end_matches("::").trim();
        let Some(close) = path[open + 1..].find('}') else {
            return Vec::new();
        };
        return path[open + 1..open + 1 + close]
            .split(',')
            .filter_map(|item| rust_use_binding(base, item.trim()))
            .collect();
    }
    rust_use_binding("", path).into_iter().collect()
}

fn rust_use_binding(base: &str, item: &str) -> Option<ImportBinding> {
    if item.is_empty() {
        return None;
    }
    let (imported, local) = split_alias(item).unwrap_or_else(|| {
        let local = item.rsplit("::").next().unwrap_or(item);
        (item, local)
    });
    let imported_path = if base.is_empty() {
        imported.to_string()
    } else {
        format!("{base}::{imported}")
    };
    let local = local.trim();
    if !looks_like_identifier(local) {
        return None;
    }
    Some(ImportBinding::target_unsupported(
        local.to_string(),
        Some(imported_path.clone()),
        Some(imported_path),
        "rust_use",
        "rust use target resolution requires crate graph and module path context",
    ))
}

fn parse_go_import_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text);
    let Some(after_import) = trimmed.strip_prefix("import") else {
        return Vec::new();
    };
    let after_import = after_import.trim();
    if after_import.starts_with('(') {
        return after_import
            .trim_start_matches('(')
            .trim_end_matches(')')
            .lines()
            .filter_map(parse_go_import_item)
            .collect();
    }
    parse_go_import_item(after_import).into_iter().collect()
}

fn parse_go_import_item(item: &str) -> Option<ImportBinding> {
    let item = item.trim().trim_end_matches(';').trim();
    let module_specifier = first_quoted_literal(item)?;
    let before_quote = item
        .split_once('"')
        .map(|(before, _)| before)
        .or_else(|| item.split_once('\'').map(|(before, _)| before))
        .unwrap_or("")
        .trim();
    let local_name = if before_quote.is_empty() {
        module_specifier
            .rsplit('/')
            .next()
            .unwrap_or(module_specifier.as_str())
            .replace('-', "_")
    } else if before_quote == "." {
        format!(
            "dot_{}",
            module_specifier
                .rsplit('/')
                .next()
                .unwrap_or(module_specifier.as_str())
                .replace('-', "_")
        )
    } else {
        before_quote.to_string()
    };
    Some(ImportBinding::target_unsupported(
        local_name,
        Some(module_specifier.clone()),
        Some(module_specifier),
        "go_import",
        "go import target resolution requires module graph context",
    ))
}

fn parse_c_include_binding(text: &str, import_kind: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text);
    include_specifier(trimmed)
        .map(|module| {
            vec![ImportBinding::preprocessor_include(
                module.clone(),
                Some(module),
                import_kind,
            )]
        })
        .unwrap_or_default()
}

fn parse_java_import_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text);
    let Some(after_import) = trimmed.strip_prefix("import ") else {
        return Vec::new();
    };
    let path = after_import.trim_start_matches("static ").trim();
    let local = path
        .rsplit('.')
        .next()
        .unwrap_or(path)
        .trim_end_matches('*')
        .trim_matches('.');
    if local.is_empty() {
        return Vec::new();
    }
    vec![ImportBinding::target_unsupported(
        local.to_string(),
        Some(path.to_string()),
        Some(path.to_string()),
        "java_import",
        "java import target resolution requires classpath context",
    )]
}

fn parse_csharp_using_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text);
    let Some(after_using) = trimmed.strip_prefix("using ") else {
        return Vec::new();
    };
    let (imported, local) = if let Some((alias, target)) = after_using.split_once('=') {
        (target.trim(), alias.trim())
    } else {
        let imported = after_using.trim();
        let local = imported.rsplit('.').next().unwrap_or(imported);
        (imported, local)
    };
    if local.is_empty() {
        return Vec::new();
    }
    vec![ImportBinding::target_unsupported(
        local.to_string(),
        Some(imported.to_string()),
        Some(imported.to_string()),
        "csharp_using",
        "csharp using target resolution requires project reference context",
    )]
}

fn parse_ruby_import_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text);
    let import_kind = if trimmed.starts_with("require_relative ") {
        "ruby_require_relative"
    } else if trimmed.starts_with("require ") {
        "ruby_require"
    } else {
        return Vec::new();
    };
    first_quoted_literal(trimmed)
        .map(|module| {
            vec![ImportBinding::target_unsupported(
                module.clone(),
                Some(module.clone()),
                Some(module),
                import_kind,
                "ruby require target resolution requires load path context",
            )]
        })
        .unwrap_or_default()
}

fn parse_php_import_bindings(text: &str) -> Vec<ImportBinding> {
    let trimmed = trim_statement(text).trim_start_matches("<?php").trim();
    if let Some(after_use) = trimmed.strip_prefix("use ") {
        return after_use
            .split(',')
            .filter_map(|item| {
                let item = item.trim();
                let (imported, local) = split_alias(item).unwrap_or_else(|| {
                    let local = item.rsplit('\\').next().unwrap_or(item);
                    (item, local)
                });
                if local.is_empty() {
                    return None;
                }
                Some(ImportBinding::target_unsupported(
                    local.to_string(),
                    Some(imported.to_string()),
                    Some(imported.to_string()),
                    "php_use",
                    "php use target resolution requires autoload context",
                ))
            })
            .collect();
    }
    if trimmed.starts_with("include") || trimmed.starts_with("require") {
        return first_quoted_literal(trimmed)
            .map(|module| {
                vec![ImportBinding::target_unsupported(
                    module.clone(),
                    Some(module.clone()),
                    Some(module),
                    "php_include",
                    "php include target resolution requires runtime include path context",
                )]
            })
            .unwrap_or_default();
    }
    Vec::new()
}

fn trim_statement(text: &str) -> &str {
    text.trim().trim_end_matches(';').trim()
}

fn strip_type_prefix(value: &str) -> &str {
    value
        .trim()
        .strip_prefix("type ")
        .unwrap_or(value.trim())
        .trim()
}

fn split_alias(item: &str) -> Option<(&str, &str)> {
    for separator in [" as ", " AS "] {
        if let Some((imported, local)) = item.split_once(separator) {
            return Some((imported.trim(), local.trim()));
        }
    }
    None
}

fn first_quoted_literal(text: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(start) = text.find(quote) {
            let after = &text[start + quote.len_utf8()..];
            if let Some(end) = after.find(quote) {
                return Some(after[..end].to_string());
            }
        }
    }
    None
}

fn include_specifier(text: &str) -> Option<String> {
    first_quoted_literal(text).or_else(|| {
        let start = text.find('<')?;
        let end = text[start + 1..].find('>')?;
        Some(text[start + 1..start + 1 + end].to_string())
    })
}

fn is_generic_export_node(language: SourceLanguage, node: Node<'_>, source: &str) -> bool {
    let kind = node.kind();
    if matches!(kind, "export_statement" | "export_declaration") {
        return true;
    }
    let Some(text) = node_text(node, source) else {
        return false;
    };
    match language {
        SourceLanguage::Rust => text.starts_with("pub "),
        SourceLanguage::Java | SourceLanguage::CSharp => text.starts_with("public "),
        SourceLanguage::Go => generic_decl_kind(language, node).is_some_and(|_| {
            generic_decl_name(node, source)
                .and_then(|name| name.chars().next())
                .is_some_and(char::is_uppercase)
        }),
        _ => false,
    }
}

fn default_export_declaration<'a>(node: Node<'a>, source: &str) -> Option<(EntityKind, Node<'a>)> {
    let text = node_text(node, source)?;
    if !text.trim_start().starts_with("export default") {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let kind = match child.kind() {
            "function_declaration" | "generator_function_declaration" => EntityKind::Function,
            "class_declaration" => EntityKind::Class,
            _ => continue,
        };
        return Some((kind, child));
    }
    None
}

fn clean_decl_name(raw: String) -> String {
    raw.trim()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
        .to_string()
}

fn looks_like_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$'))
}

fn simple_mutation_method_receiver(callee_label: &str) -> Option<(&str, &str)> {
    let (receiver, method) = callee_label.rsplit_once('.')?;
    if !looks_like_identifier(receiver) || !looks_like_identifier(method) {
        return None;
    }
    if !obvious_mutation_method(method) {
        return None;
    }
    Some((receiver, method))
}

fn obvious_mutation_method(method: &str) -> bool {
    matches!(
        method,
        "push"
            | "append"
            | "set"
            | "add"
            | "delete"
            | "clear"
            | "pop"
            | "shift"
            | "unshift"
            | "splice"
            | "sort"
            | "reverse"
    )
}

fn qualify(scope_name: &str, name: &str) -> String {
    if scope_name.is_empty() {
        name.to_string()
    } else {
        format!("{scope_name}.{name}")
    }
}

fn module_name_for_path(path: &str) -> String {
    let normalized = normalize_repo_relative_path(path);
    normalized
        .rsplit_once('.')
        .map(|(without_ext, _)| without_ext)
        .unwrap_or(&normalized)
        .replace('/', "::")
}

pub fn content_hash(source: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use codegraph_core::{
        Entity, EntityKind, Exactness, MicroDerivationKind, MicroEdgeKind, MicroExactness,
        MicroNodeKind, MicroSourceRole, RelationKind, SourceSpan,
    };
    use codegraph_store::{GraphStore, SqliteGraphStore};

    use super::{
        detect_language, detect_language_with_source, discover_mvp4_typescript_scopes,
        emit_mvp4_2_typescript_micro_edge_candidates,
        emit_mvp4_2b_typescript_value_use_persistable_candidates,
        emit_mvp4_typescript_core_declaration_candidates,
        emit_mvp4_typescript_in_memory_first_slice_inventory,
        emit_mvp4_typescript_in_memory_first_slice_inventory_with_policy,
        emit_mvp4_typescript_local_flows_to_in_memory_candidates,
        emit_mvp4_typescript_local_reads_in_memory_candidates,
        emit_mvp4_typescript_local_returns_to_in_memory_candidates,
        emit_mvp4_typescript_local_returns_to_in_memory_candidates_with_policy,
        emit_mvp4_typescript_local_writes_in_memory_candidates,
        emit_mvp4_typescript_micro_flow_extraction_context,
        emit_mvp4_typescript_operation_site_candidates,
        emit_mvp4_typescript_property_access_candidates,
        emit_mvp4_typescript_resolved_value_use_candidates,
        emit_mvp4_typescript_value_use_candidates, extract_basic_entities,
        mvp4_typescript_first_slice_cap_policy, normalize_repo_relative_path, LanguageFrontend,
        LanguageParser, Mvp4TypeScriptMicroNodeCapPolicy, SourceLanguage, TreeSitterParser,
    };

    const JS_FIXTURE: &str = include_str!("../fixtures/basic.js");
    const TS_FIXTURE: &str = include_str!("../fixtures/basic.ts");
    const TSX_FIXTURE: &str = include_str!("../fixtures/component.tsx");
    const SIMPLE_FUNCTION: &str = include_str!("../fixtures/simple_function.ts");
    const CLASS_FIXTURE: &str = include_str!("../fixtures/class_with_methods.ts");
    const IMPORT_EXPORT_FIXTURE: &str = include_str!("../fixtures/imports_exports.ts");
    const NESTED_FUNCTIONS: &str = include_str!("../fixtures/nested_functions.ts");
    const CORE_RELATIONS: &str = include_str!("../fixtures/core_relations.ts");
    const ROUTE_AUTH_SECURITY: &str = include_str!("../fixtures/route_auth_security.ts");
    const EVENTS_ASYNC: &str = include_str!("../fixtures/events_async.ts");
    const MIGRATION: &str = include_str!("../fixtures/migration.ts");
    const AUTH_SPEC: &str = include_str!("../fixtures/auth.spec.ts");

    const PHASE_07_RELATIONS: &[RelationKind] = &[
        RelationKind::Authorizes,
        RelationKind::ChecksRole,
        RelationKind::ChecksPermission,
        RelationKind::Sanitizes,
        RelationKind::Validates,
        RelationKind::Exposes,
        RelationKind::TrustBoundary,
        RelationKind::SourceOfTaint,
        RelationKind::SinksTo,
        RelationKind::Publishes,
        RelationKind::Emits,
        RelationKind::Consumes,
        RelationKind::ListensTo,
        RelationKind::SubscribesTo,
        RelationKind::Handles,
        RelationKind::Spawns,
        RelationKind::Awaits,
        RelationKind::Migrates,
        RelationKind::ReadsTable,
        RelationKind::WritesTable,
        RelationKind::AltersColumn,
        RelationKind::DependsOnSchema,
        RelationKind::Tests,
        RelationKind::Asserts,
        RelationKind::Mocks,
        RelationKind::Stubs,
        RelationKind::Covers,
        RelationKind::FixturesFor,
    ];

    fn parser() -> TreeSitterParser {
        TreeSitterParser
    }

    fn parsed(path: &str, source: &str) -> super::ParsedFile {
        match parser().parse(path, source) {
            Ok(Some(parsed)) => parsed,
            Ok(None) => panic!("expected supported parser for {path}"),
            Err(error) => panic!("expected parse success for {path}, got {error}"),
        }
    }

    fn extraction(path: &str, source: &str) -> super::BasicExtraction {
        let parsed = parsed(path, source);
        extract_basic_entities(&parsed, source)
    }

    fn source_line_count(source: &str) -> u32 {
        source.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1
    }

    fn source_line_len(source: &str, one_based_line: u32) -> usize {
        source
            .split('\n')
            .nth(one_based_line.saturating_sub(1) as usize)
            .unwrap_or("")
            .trim_end_matches('\r')
            .len()
    }

    fn assert_span_inside_source(path: &str, source: &str, span: &SourceSpan) {
        assert_eq!(span.repo_relative_path, path, "{span}");
        assert!(span.start_line >= 1, "{span}");
        assert!(span.end_line >= span.start_line, "{span}");
        assert!(span.end_line <= source_line_count(source), "{span}");
        if let Some(column) = span.start_column {
            assert!(column >= 1, "{span}");
            assert!(
                column as usize <= source_line_len(source, span.start_line) + 1,
                "{span}"
            );
        }
        if let Some(column) = span.end_column {
            assert!(column >= 1, "{span}");
            assert!(
                column as usize <= source_line_len(source, span.end_line) + 1,
                "{span}"
            );
        }
    }

    fn assert_extraction_spans_inside_source(
        path: &str,
        source: &str,
        extraction: &super::BasicExtraction,
    ) {
        for entity in &extraction.entities {
            let span = entity
                .source_span
                .as_ref()
                .unwrap_or_else(|| panic!("entity {} missing span", entity.name));
            assert_span_inside_source(path, source, span);
        }
        for edge in &extraction.edges {
            assert_span_inside_source(path, source, &edge.source_span);
        }
    }

    fn assert_phase_07_relation(extraction: &super::BasicExtraction, relation: RelationKind) {
        let matching = extraction
            .edges
            .iter()
            .filter(|edge| edge.relation == relation)
            .collect::<Vec<_>>();
        assert!(!matching.is_empty(), "missing {relation}");
        for edge in matching {
            assert!(
                edge.exactness == Exactness::StaticHeuristic
                    || super::is_proof_grade_exactness(edge.exactness),
                "{relation}"
            );
            assert!(edge.confidence <= 1.0, "{relation}");
            if edge.exactness == Exactness::StaticHeuristic {
                assert_eq!(edge.extractor, "tree-sitter-extended-heuristic");
                assert_eq!(
                    edge.metadata.get("phase").and_then(|value| value.as_str()),
                    Some("07")
                );
                assert!(edge.metadata.contains_key("pattern"), "{relation}");
                assert!(edge.metadata.contains_key("framework"), "{relation}");
            }
            assert!(
                !edge.source_span.repo_relative_path.is_empty(),
                "{relation}"
            );
        }
    }

    fn metadata_str<'a>(entity: &'a Entity, key: &str) -> Option<&'a str> {
        entity.metadata.get(key).and_then(serde_json::Value::as_str)
    }

    fn mvp4_scope_report(path: &str, source: &str) -> super::Mvp4TypeScriptScopeDiscoveryReport {
        let parsed = parsed(path, source);
        discover_mvp4_typescript_scopes(&parsed, source)
    }

    fn mvp4_core_declaration_report(
        path: &str,
        source: &str,
    ) -> super::Mvp4TypeScriptCoreDeclarationCandidateReport {
        let parsed = parsed(path, source);
        emit_mvp4_typescript_core_declaration_candidates(&parsed, source)
    }

    fn mvp4_operation_site_report(
        path: &str,
        source: &str,
    ) -> super::Mvp4TypeScriptOperationSiteCandidateReport {
        let parsed = parsed(path, source);
        emit_mvp4_typescript_operation_site_candidates(&parsed, source)
    }

    fn mvp4_property_access_report(
        path: &str,
        source: &str,
    ) -> super::Mvp4TypeScriptPropertyAccessCandidateReport {
        let parsed = parsed(path, source);
        emit_mvp4_typescript_property_access_candidates(&parsed, source)
    }

    fn mvp4_value_use_report(
        path: &str,
        source: &str,
    ) -> super::Mvp4TypeScriptValueUseCandidateReport {
        let parsed = parsed(path, source);
        emit_mvp4_typescript_value_use_candidates(&parsed, source)
    }

    #[test]
    fn mvp4_value_use_captures_reads_and_excludes_declarations_writes_and_callees() {
        let source = concat!(
            "export function compute(seed: number) {\n",
            "  const base = seed;\n",
            "  let total = base + offset;\n",
            "  total = base;\n",
            "  return helper(total);\n",
            "}\n",
        );
        let report = mvp4_value_use_report("src/compute.ts", source);
        assert!(report.eligible, "report: {report:?}");
        assert_eq!(report.binding_overclaim_count, 0);
        assert_eq!(report.source_span_failures, 0);
        assert_eq!(report.duplicate_candidate_count, 0);

        let count_label = |needle: &str| {
            report
                .candidates
                .iter()
                .filter(|candidate| candidate.name_or_literal == needle)
                .count()
        };

        // Reads of value bindings are captured, one per occurrence.
        assert_eq!(count_label("seed"), 1, "expected one read of `seed`");
        // `base` is read on the binary RHS (`base + offset`) and the assignment
        // RHS (`total = base`) — its declaration occurrence is not a read.
        assert_eq!(count_label("base"), 2, "expected two reads of `base`");
        assert!(count_label("offset") >= 1, "expected read of free `offset`");
        // `total` is read as the `helper(total)` argument, but NOT at its
        // declaration or as the assignment LHS write target.
        assert!(count_label("total") >= 1, "expected a read of `total`");
        // The callee identifier belongs to the CallSite, not a value use.
        assert_eq!(count_label("helper"), 0, "callee must not be a value use");
        // Only ValueUse candidates are emitted by this extractor.
        assert!(report
            .candidates
            .iter()
            .all(|candidate| candidate.node_kind == super::MicroNodeKind::ValueUse));
        // Existence-only: never a resolved binding, always exact when clean.
        assert!(report.candidates.iter().all(|candidate| {
            candidate.symbol_binding_id.is_none()
                && candidate.exactness == super::MicroExactness::Exact
                && candidate.claimability == "claimable_source_spanned_value_use_existence"
        }));
    }

    #[test]
    fn mvp4_value_use_excluded_for_non_production_and_non_ts() {
        let ts_test = mvp4_value_use_report(
            "src/compute.test.ts",
            "export function f(a: number) { return a; }\n",
        );
        assert!(!ts_test.eligible);
        assert!(ts_test.candidates.is_empty());

        let js = mvp4_value_use_report("src/compute.js", "export function f(a) { return a; }\n");
        assert!(!js.eligible);
        assert!(js.candidates.is_empty());
    }

    #[test]
    fn mvp4_value_use_occurrence_indices_and_function_separation_are_stable() {
        let source = concat!(
            "export function alpha(seed: number) {\n",
            "  const a = seed + seed;\n",
            "  return a + seed;\n",
            "}\n",
            "export function beta(seed: number) {\n",
            "  return seed;\n",
            "}\n",
        );
        let report = mvp4_value_use_report("src/sep.ts", source);
        assert!(report.eligible);
        assert_eq!(report.duplicate_candidate_count, 0, "ids must be unique");
        assert_eq!(report.source_span_failures, 0);

        // `seed` is read 3x in alpha (`seed + seed`, then `a + seed`) and 1x in beta.
        let seed_reads: Vec<&super::Mvp4TypeScriptMicroNodeCandidate> = report
            .candidates
            .iter()
            .filter(|candidate| candidate.name_or_literal == "seed")
            .collect();
        assert_eq!(seed_reads.len(), 4, "expected 4 reads of `seed`");

        // alpha and beta are distinct enclosing functions.
        let functions: std::collections::BTreeSet<&str> = seed_reads
            .iter()
            .map(|candidate| candidate.enclosing_function_identity.as_str())
            .collect();
        assert_eq!(functions.len(), 2, "alpha and beta must be separated");

        // The three alpha `seed` reads carry distinct occurrence indices 0,1,2.
        let alpha_identity = report
            .candidates
            .iter()
            .find(|candidate| candidate.name_or_literal == "a")
            .map(|candidate| candidate.enclosing_function_identity.clone())
            .expect("alpha read of `a`");
        let mut alpha_seed_occurrences: Vec<u32> = seed_reads
            .iter()
            .filter(|candidate| candidate.enclosing_function_identity == alpha_identity)
            .map(|candidate| candidate.occurrence_index)
            .collect();
        alpha_seed_occurrences.sort_unstable();
        assert_eq!(alpha_seed_occurrences, vec![0, 1, 2]);

        // Emission is deterministic across runs.
        let again = mvp4_value_use_report("src/sep.ts", source);
        let ids_first: Vec<&str> = report
            .candidates
            .iter()
            .map(|candidate| candidate.micro_node_id.as_str())
            .collect();
        let ids_again: Vec<&str> = again
            .candidates
            .iter()
            .map(|candidate| candidate.micro_node_id.as_str())
            .collect();
        assert_eq!(ids_first, ids_again, "ids must be stable across runs");
    }

    #[test]
    fn mvp4_value_use_parse_recovery_downgrades_to_unknown() {
        // `const y = ;` is a syntax error; the file recovers but is flagged.
        let source = concat!(
            "export function f(a: number) {\n",
            "  const x = a;\n",
            "  const y = ;\n",
            "  return x;\n",
            "}\n",
        );
        let report = mvp4_value_use_report("src/recover.ts", source);
        assert!(
            report.eligible,
            "recoverable file stays eligible: {report:?}"
        );
        assert!(
            !report.candidates.is_empty(),
            "expected at least one recovered read: {report:?}"
        );
        assert!(report.candidates.iter().all(|candidate| {
            candidate.parse_recovery
                && candidate.exactness == super::MicroExactness::Unknown
                && candidate.claimability == "non_claimable_recovery_candidate"
        }));
        // Recovery never fabricates a resolved binding.
        assert_eq!(report.binding_overclaim_count, 0);
    }

    #[test]
    fn mvp4_local_binding_resolver_handles_shadowing_params_and_free_vars() {
        let source = concat!(
            "export function outer(seed: number) {\n", // L1
            "  const value = seed;\n",                 // L2: `seed` read -> parameter
            "  {\n",                                   // L3
            "    const value = 2;\n",                  // L4: inner shadow declaration
            "    log(value);\n",                       // L5: `value` read -> inner const
            "  }\n",                                   // L6
            "  return value + free;\n",                // L7: `value` -> outer; `free` -> unresolved
            "}\n",                                     // L8
        );
        let parsed = parsed("src/resolve.ts", source);
        let scope_report = discover_mvp4_typescript_scopes(&parsed, source);
        let function = scope_report
            .functions
            .iter()
            .find(|function| function.function_name == "outer")
            .expect("outer scope");
        let uses = emit_mvp4_typescript_value_use_candidates(&parsed, source);

        let resolve_at = |label: &str, line: u32| {
            let occurrence = uses
                .candidates
                .iter()
                .find(|candidate| {
                    candidate.name_or_literal == label && candidate.source_span.start_line == line
                })
                .unwrap_or_else(|| panic!("no use of `{label}` at line {line}"));
            super::resolve_mvp4_ts_local_binding(
                function,
                &occurrence.name_or_literal,
                &occurrence.source_span,
            )
        };

        // `seed` read resolves to the parameter (visible across the whole body).
        let seed = resolve_at("seed", 2);
        assert_eq!(
            seed.status,
            super::Mvp4TypeScriptBindingResolutionStatus::Resolved
        );
        assert_eq!(seed.binding_kind.as_deref(), Some("parameter"));

        // `value` inside the inner block resolves to the inner const (line 4).
        let inner_value = resolve_at("value", 5);
        assert_eq!(
            inner_value.status,
            super::Mvp4TypeScriptBindingResolutionStatus::Resolved
        );
        assert_eq!(
            inner_value
                .binding_source_span
                .as_ref()
                .map(|span| span.start_line),
            Some(4)
        );

        // `value` after the block resolves to the outer const (line 2) — shadowing
        // is correctly scoped, not leaked.
        let outer_value = resolve_at("value", 7);
        assert_eq!(
            outer_value.status,
            super::Mvp4TypeScriptBindingResolutionStatus::Resolved
        );
        assert_eq!(
            outer_value
                .binding_source_span
                .as_ref()
                .map(|span| span.start_line),
            Some(2)
        );

        // The two `value` declarations have distinct binding ids.
        assert_ne!(inner_value.symbol_binding_id, outer_value.symbol_binding_id);

        // A free variable resolves to nothing — never a guessed binding.
        let free = resolve_at("free", 7);
        assert_eq!(
            free.status,
            super::Mvp4TypeScriptBindingResolutionStatus::Unresolved
        );
        assert_eq!(
            free.unresolved_reason.as_deref(),
            Some("no_enclosing_declaration")
        );
        assert!(free.symbol_binding_id.is_none());
    }

    #[test]
    fn mvp4_resolved_value_use_sets_binding_ids_and_records_unresolved() {
        let source = concat!(
            "export function outer(seed: number) {\n", // L1
            "  const base = seed;\n",                  // L2: seed -> param
            "  const total = base + base;\n",          // L3: base x2 -> local
            "  return total + free;\n",                // L4: total -> local; free -> unresolved
            "}\n",                                     // L5
        );
        let parsed = parsed("src/resolved.ts", source);
        let report = emit_mvp4_typescript_resolved_value_use_candidates(&parsed, source);
        assert!(report.eligible);

        // seed(1) + base(2) + total(1) resolve; free(1) does not.
        assert_eq!(report.resolved_count, 4, "{report:?}");
        assert_eq!(report.unresolved_count, 1, "{report:?}");
        assert_eq!(
            report.unresolved_by_reason.get("no_enclosing_declaration"),
            Some(&1)
        );

        // Resolved reads carry a binding id; the free variable does not.
        for candidate in &report.candidates {
            if candidate.name_or_literal == "free" {
                assert!(candidate.symbol_binding_id.is_none());
            } else {
                assert!(
                    candidate.symbol_binding_id.is_some(),
                    "expected binding id for `{}`",
                    candidate.name_or_literal
                );
            }
        }

        // Both `base` reads resolve to the SAME declaration → one binding id.
        let base_ids: std::collections::BTreeSet<&str> = report
            .candidates
            .iter()
            .filter(|candidate| candidate.name_or_literal == "base")
            .filter_map(|candidate| candidate.symbol_binding_id.as_deref())
            .collect();
        assert_eq!(base_ids.len(), 1, "both `base` reads share one binding id");
    }

    #[test]
    fn mvp4_2b_value_use_persistable_candidates_are_capped_production_exact() {
        let source = concat!(
            "export function outer(seed: number) {\n",
            "  const base = seed;\n",
            "  return base + seed;\n",
            "}\n",
        );
        let candidates = emit_mvp4_2b_typescript_value_use_persistable_candidates(
            &parsed("src/persist_reads.ts", source),
            source,
        );
        // seed(L2) + base(L3) + seed(L3) = 3 production exact existence reads.
        assert_eq!(candidates.len(), 3, "{candidates:?}");
        assert!(candidates.iter().all(|candidate| {
            candidate.node_kind == super::MicroNodeKind::ValueUse
                && candidate.source_role == super::MicroSourceRole::Production
                && !candidate.parse_recovery
                && candidate.symbol_binding_id.is_none()
                && candidate.claimability == "claimable_source_spanned_value_use_existence"
        }));

        // Non-production files yield nothing.
        let test_source = "export function f(a: number) { return a; }\n";
        let test_file = emit_mvp4_2b_typescript_value_use_persistable_candidates(
            &parsed("src/outer.test.ts", test_source),
            test_source,
        );
        assert!(test_file.is_empty());
    }

    #[test]
    fn mvp4_local_reads_emits_exact_edges_to_resolved_bindings() {
        let source = concat!(
            "export function outer(seed: number) {\n", // L1
            "  const base = seed;\n",                  // L2: seed read -> param; base decl
            "  return base + free;\n",                 // L3: base read -> local; free unresolved
            "}\n",                                     // L4
        );
        let parsed = parsed("src/reads.ts", source);
        let report = emit_mvp4_typescript_local_reads_in_memory_candidates(&parsed, source);
        assert!(report.eligible);
        assert!(!report.flow_proof_activated);
        assert!(!report.mutation_proof_activated);
        assert_eq!(report.flow_semantic_overclaim_count, 0);

        // seed -> parameter, base -> local: two exact edges; free is unresolved.
        assert_eq!(report.resolved_read_count, 2, "{report:?}");
        assert_eq!(report.unresolved_read_count, 1, "{report:?}");
        assert_eq!(report.candidates.len(), 2, "{report:?}");
        assert_eq!(report.omitted_edge_count, 0);
        assert_eq!(report.duplicate_candidate_count, 0);

        for edge in &report.candidates {
            assert_eq!(edge.micro_edge_kind, super::MicroEdgeKind::LocalReads);
            assert_eq!(edge.exactness, super::MicroExactness::Exact);
            assert!(edge.claimability.starts_with("claimable_"));
            assert!(!edge.head_micro_node_id.is_empty());
            assert!(!edge.tail_micro_node_id.is_empty());
            // Proof boundary: a read edge never claims flow or mutation.
            assert!(edge
                .provenance
                .limitations
                .iter()
                .any(|limitation| limitation == "does_not_prove_value_flow"));
            assert!(edge
                .provenance
                .limitations
                .iter()
                .any(|limitation| limitation == "does_not_activate_mutation_proof_or_flow_proof"));
        }

        // `seed` and `base` resolve to distinct declaration micro-nodes.
        let tails: std::collections::BTreeSet<&str> = report
            .candidates
            .iter()
            .map(|edge| edge.tail_micro_node_id.as_str())
            .collect();
        assert_eq!(tails.len(), 2, "distinct binding endpoints");
    }

    #[test]
    fn mvp4_local_writes_emits_exact_edges_to_resolved_targets() {
        let source = concat!(
            "export function outer(seed: number) {\n", // L1
            "  let total = 0;\n",                      // L2: total declaration (not a write edge)
            "  total = seed;\n",                       // L3: write `total` -> local
            "  globalCounter = total;\n",              // L4: write `globalCounter` -> unresolved
            "  return total;\n",                       // L5
            "}\n",                                     // L6
        );
        let parsed = parsed("src/writes.ts", source);
        let report = emit_mvp4_typescript_local_writes_in_memory_candidates(&parsed, source);
        assert!(report.eligible);
        assert!(!report.flow_proof_activated);
        assert!(!report.mutation_proof_activated);
        assert_eq!(report.flow_semantic_overclaim_count, 0);

        // `total = seed` resolves to the local `total`; `globalCounter = total`
        // has no local declaration, so no edge — never guessed.
        assert_eq!(report.resolved_write_count, 1, "{report:?}");
        assert_eq!(report.unresolved_write_count, 1, "{report:?}");
        assert_eq!(report.candidates.len(), 1, "{report:?}");
        assert_eq!(report.omitted_edge_count, 0);

        let edge = &report.candidates[0];
        assert_eq!(edge.micro_edge_kind, super::MicroEdgeKind::LocalWrites);
        assert_eq!(edge.exactness, super::MicroExactness::Exact);
        assert!(edge.claimability.starts_with("claimable_"));
        assert!(!edge.head_micro_node_id.is_empty());
        assert!(!edge.tail_micro_node_id.is_empty());
        // Proof boundary: a write edge never claims flow or mutation semantics.
        assert!(edge
            .provenance
            .limitations
            .iter()
            .any(|limitation| limitation == "does_not_prove_value_flow"));
        assert!(edge
            .provenance
            .limitations
            .iter()
            .any(|limitation| limitation == "does_not_prove_mutation_semantics"));
    }

    #[test]
    fn mvp4_local_flows_to_derives_initializer_and_assignment_chains_with_provenance() {
        let source = concat!(
            "export function update(seed: number, extra: number) {\n", // L1
            "  const base = seed;\n",                                  // L2: seed -> base
            "  let total = base;\n",                                   // L3: base -> total
            "  total = base + extra;\n",                               // L4: base/extra -> total
            "  return total;\n",                                       // L5
            "}\n",                                                     // L6
        );
        let parsed = parsed("src/flows.ts", source);
        let report = emit_mvp4_typescript_local_flows_to_in_memory_candidates(&parsed, source);
        assert!(report.eligible, "report: {report:?}");
        assert!(!report.flow_proof_activated);
        assert!(!report.mutation_proof_activated);
        assert_eq!(report.local_flow_packet_rows, 0);
        assert_eq!(report.production_micro_edge_rows, 0);
        assert_eq!(report.exactness_overclaim_count, 0, "{report:?}");
        assert_eq!(report.duplicate_candidate_count, 0, "{report:?}");
        assert_eq!(report.claimable_edge_missing_provenance_count, 0);
        assert_eq!(report.claimable_edge_missing_span_count, 0);

        // Initializers and reassignments both derive bounded local flow edges,
        // but only as parser in-memory candidates.
        assert!(
            report.initializer_flow_count >= 2,
            "expected seed->base and base->total initializer flows: {report:?}"
        );
        assert!(
            report.assignment_flow_count >= 2,
            "expected base/extra->total assignment flows: {report:?}"
        );

        let inventory = emit_mvp4_typescript_in_memory_first_slice_inventory(&parsed, source);
        let nodes_by_id = inventory
            .candidates
            .iter()
            .map(|candidate| (candidate.micro_node_id.as_str(), candidate))
            .collect::<BTreeMap<_, _>>();
        let mut endpoint_pairs = BTreeSet::new();
        for edge in &report.candidates {
            assert_eq!(edge.micro_edge_kind, MicroEdgeKind::LocalFlowsTo);
            assert_eq!(edge.exactness, MicroExactness::DerivedWithProvenance);
            assert_eq!(
                edge.provenance.exactness,
                MicroExactness::DerivedWithProvenance
            );
            assert_eq!(
                edge.provenance.derivation_kind,
                MicroDerivationKind::LocalAssignmentChainDerivation
            );
            assert_eq!(
                edge.provenance.extractor_or_adapter_version,
                "mvp4.2b-typescript-local-flows-to-v1"
            );
            assert_eq!(
                edge.claimability,
                "claimable_source_spanned_derived_local_value_flow"
            );
            assert!(
                edge.provenance.source_fact_ids.len() >= 2,
                "derived flow must cite base read/write facts: {edge:?}"
            );
            assert!(edge
                .provenance
                .limitations
                .iter()
                .any(|limitation| limitation == "function_local_only"));
            assert!(edge
                .provenance
                .limitations
                .iter()
                .any(|limitation| limitation == "does_not_activate_mutation_proof_or_flow_proof"));

            let head = nodes_by_id
                .get(edge.head_micro_node_id.as_str())
                .expect("source binding node");
            let tail = nodes_by_id
                .get(edge.tail_micro_node_id.as_str())
                .expect("target binding node");
            assert!(matches!(
                head.node_kind,
                MicroNodeKind::Parameter | MicroNodeKind::LocalBinding
            ));
            assert!(matches!(
                tail.node_kind,
                MicroNodeKind::Parameter | MicroNodeKind::LocalBinding
            ));
            endpoint_pairs.insert((head.name_or_literal.as_str(), tail.name_or_literal.as_str()));
        }

        assert!(
            endpoint_pairs.contains(&("seed", "base")),
            "{endpoint_pairs:?}"
        );
        assert!(
            endpoint_pairs.contains(&("base", "total")),
            "{endpoint_pairs:?}"
        );
        assert!(
            endpoint_pairs.contains(&("extra", "total")),
            "{endpoint_pairs:?}"
        );
    }

    #[test]
    fn mvp4_local_flows_to_excludes_non_production_and_does_not_activate_packets() {
        let source = "export function f(seed: number) { const base = seed; return base; }\n";
        let parsed = parsed("src/flows.test.ts", source);
        let report = emit_mvp4_typescript_local_flows_to_in_memory_candidates(&parsed, source);
        assert!(!report.eligible, "{report:?}");
        assert!(report.candidates.is_empty());
        assert_eq!(report.production_micro_edge_rows, 0);
        assert_eq!(report.local_flow_packet_rows, 0);
        assert!(!report.flow_proof_activated);
        assert!(!report.mutation_proof_activated);
    }

    #[test]
    fn mvp4_micro_flow_extraction_context_matches_standalone_outputs() {
        let source = "\
export function handle(seed: number, extra: number) {
  const base = seed;
  let total = base;
  total = total + extra;
  return total;
}
";
        let parsed = parsed("src/context_flow.ts", source);

        let context = emit_mvp4_typescript_micro_flow_extraction_context(&parsed, source);
        let inventory = emit_mvp4_typescript_in_memory_first_slice_inventory(&parsed, source);
        let persistable_value_uses =
            emit_mvp4_2b_typescript_value_use_persistable_candidates(&parsed, source);
        let micro_edges = emit_mvp4_2_typescript_micro_edge_candidates(&parsed, source);

        assert_eq!(context.inventory, inventory);
        assert_eq!(context.persistable_value_uses, persistable_value_uses);
        assert_eq!(context.micro_edges, micro_edges);
        assert!(
            context
                .micro_edges
                .candidates
                .iter()
                .any(|edge| edge.micro_edge_kind == MicroEdgeKind::LocalFlowsTo),
            "{:?}",
            context.micro_edges
        );
    }

    /// MVP4.2b **P2** (FYI66c): measured per-file micro-edge EXTRACTION baseline.
    /// `#[ignore]` because it is a MEASUREMENT, not an assertion gate — it exists
    /// so the MVP4.3 packet caps (`max_packet_steps/bytes/snippets`, currently
    /// `null` by design) are EVIDENCE-set, not invented. Run on the WSL gate:
    ///   cargo test -p codegraph-parser mvp4_2b_p2_extraction_latency_baseline \
    ///       -- --ignored --nocapture
    /// It times the production extraction wrapper
    /// (`emit_mvp4_2_typescript_micro_edge_candidates`) across a size/density
    /// matrix and reports: extraction ms, edge counts by kind, the per-function
    /// LOCAL_FLOWS_TO step count (= the driver of `max_packet_steps`, since a
    /// packet is an ordered flow chain per function), and a conservative
    /// per-function serialized-bytes upper bound for `max_packet_bytes`
    /// (real packets surface via handles, §8.3, so this OVER-states packet bytes).
    #[test]
    #[ignore]
    fn mvp4_2b_p2_extraction_latency_baseline() {
        use std::time::{Duration, Instant};

        // Densest realistic flow shape: every body statement is a resolver-proven
        // read->write off the params, so per-function flow-step counts here
        // UPPER-BOUND what an ordinary function produces.
        fn gen_ts(functions: usize, chain_depth: usize) -> String {
            let mut s = String::new();
            for f in 0..functions {
                s.push_str(&format!(
                    "export function fn_{f}(seed: number, extra: number) {{\n"
                ));
                s.push_str("  let acc = seed;\n");
                for d in 0..chain_depth {
                    s.push_str(&format!("  acc = acc + seed + extra; // step {d}\n"));
                }
                s.push_str("  const out = acc;\n");
                s.push_str("  return out;\n");
                s.push_str("}\n");
            }
            s
        }

        struct Cell {
            label: &'static str,
            functions: usize,
            chain_depth: usize,
        }
        let matrix = [
            Cell {
                label: "small",
                functions: 1,
                chain_depth: 5,
            },
            Cell {
                label: "medium",
                functions: 10,
                chain_depth: 10,
            },
            Cell {
                label: "large",
                functions: 50,
                chain_depth: 20,
            },
            Cell {
                label: "dense-fn",
                functions: 1,
                chain_depth: 200,
            },
            Cell {
                label: "xl-file",
                functions: 200,
                chain_depth: 20,
            },
        ];

        println!("\nMVP4.2b P2 micro-edge extraction baseline (production wrapper)");
        println!(
            "{:<9} {:>9} {:>5} {:>10} {:>7} {:>7} {:>7} {:>7} {:>13} {:>14}",
            "cell",
            "bytes",
            "fns",
            "extr_ms",
            "edges",
            "reads",
            "writes",
            "flows",
            "max_fn_flows",
            "max_fn_json_B"
        );

        let mut global_max_fn_flows = 0usize;
        let mut global_max_fn_bytes = 0usize;
        let mut slowest_ms = 0.0f64;
        for cell in &matrix {
            let source = gen_ts(cell.functions, cell.chain_depth);
            let parsed = parsed("src/p2_bench.ts", &source);

            // Min over a few iterations to cut scheduler noise.
            let mut report = emit_mvp4_2_typescript_micro_edge_candidates(&parsed, &source);
            let mut best = Duration::from_secs(3600);
            for _ in 0..5 {
                let t = Instant::now();
                report = emit_mvp4_2_typescript_micro_edge_candidates(&parsed, &source);
                let dt = t.elapsed();
                if dt < best {
                    best = dt;
                }
            }

            let mut reads = 0usize;
            let mut writes = 0usize;
            let mut flows = 0usize;
            let mut flows_by_fn: BTreeMap<String, usize> = BTreeMap::new();
            let mut flow_bytes_by_fn: BTreeMap<String, usize> = BTreeMap::new();
            for c in &report.candidates {
                match c.micro_edge_kind {
                    MicroEdgeKind::LocalReads => reads += 1,
                    MicroEdgeKind::LocalWrites => writes += 1,
                    MicroEdgeKind::LocalFlowsTo => {
                        flows += 1;
                        *flows_by_fn.entry(c.function_identity.clone()).or_default() += 1;
                        let bytes = serde_json::to_vec(c).map(|v| v.len()).unwrap_or(0);
                        *flow_bytes_by_fn
                            .entry(c.function_identity.clone())
                            .or_default() += bytes;
                    }
                    _ => {}
                }
            }
            let max_fn_flows = flows_by_fn.values().copied().max().unwrap_or(0);
            let max_fn_bytes = flow_bytes_by_fn.values().copied().max().unwrap_or(0);
            let ms = best.as_secs_f64() * 1000.0;
            global_max_fn_flows = global_max_fn_flows.max(max_fn_flows);
            global_max_fn_bytes = global_max_fn_bytes.max(max_fn_bytes);
            if ms > slowest_ms {
                slowest_ms = ms;
            }

            println!(
                "{:<9} {:>9} {:>5} {:>10.3} {:>7} {:>7} {:>7} {:>7} {:>13} {:>14}",
                cell.label,
                source.len(),
                cell.functions,
                ms,
                report.candidates.len(),
                reads,
                writes,
                flows,
                max_fn_flows,
                max_fn_bytes
            );

            // Generous blowup catch only — this is a measurement, not a budget gate.
            // NOTE (P2 finding 2026-06-26): extraction scales ~quadratically
            // (large->xl = 4x bytes but 14.5x ms); the xl-file cell ~39 s is the
            // evidence behind the O(n^2) flows-to read-matching fix.
            assert!(
                best.as_secs_f64() < 120.0,
                "extraction pathologically slow for {}: {ms:.1} ms",
                cell.label
            );
        }

        // Stage breakdown on the worst cell to isolate the dominant cost.
        {
            let source = gen_ts(200, 20);
            let parsed = parsed("src/p2_bench.ts", &source);
            let bench = |label: &str, f: &dyn Fn()| {
                let mut best = Duration::from_secs(3600);
                for _ in 0..3 {
                    let t = Instant::now();
                    f();
                    let dt = t.elapsed();
                    if dt < best {
                        best = dt;
                    }
                }
                println!(
                    "  stage {:<26} {:>10.3} ms",
                    label,
                    best.as_secs_f64() * 1000.0
                );
            };
            println!("\nP2 stage breakdown (xl-file: 200 fns, single call each):");
            bench("discover_scopes", &|| {
                let _ = discover_mvp4_typescript_scopes(&parsed, &source);
            });
            bench("inventory", &|| {
                let _ = emit_mvp4_typescript_in_memory_first_slice_inventory(&parsed, &source);
            });
            bench("value_use_persistable", &|| {
                let _ = emit_mvp4_2b_typescript_value_use_persistable_candidates(&parsed, &source);
            });
            bench("returns_to", &|| {
                let _ =
                    emit_mvp4_typescript_local_returns_to_in_memory_candidates(&parsed, &source);
            });
            bench("reads", &|| {
                let _ = emit_mvp4_typescript_local_reads_in_memory_candidates(&parsed, &source);
            });
            bench("writes", &|| {
                let _ = emit_mvp4_typescript_local_writes_in_memory_candidates(&parsed, &source);
            });
            bench("flows", &|| {
                let _ = emit_mvp4_typescript_local_flows_to_in_memory_candidates(&parsed, &source);
            });
            bench("wrapper(all)", &|| {
                let _ = emit_mvp4_2_typescript_micro_edge_candidates(&parsed, &source);
            });
        }

        println!("\nDerived MVP4.3 cap candidates (evidence-set, NOT yet applied):");
        println!(
            "  max_packet_steps   >= observed max per-function LOCAL_FLOWS_TO = {global_max_fn_flows}"
        );
        println!("  max_packet_snippets ~= max_packet_steps (one source snippet per flow step)");
        println!(
            "  max_packet_bytes    < per-function serialized-flow upper bound = {global_max_fn_bytes} B (handles shrink this)"
        );
        println!("  slowest matrix-cell extraction = {slowest_ms:.3} ms\n");
    }

    fn mvp4_in_memory_inventory_report(
        path: &str,
        source: &str,
    ) -> super::Mvp4TypeScriptInMemoryInventoryReport {
        let parsed = parsed(path, source);
        emit_mvp4_typescript_in_memory_first_slice_inventory(&parsed, source)
    }

    fn mvp4_in_memory_inventory_report_with_policy(
        path: &str,
        source: &str,
        policy: &Mvp4TypeScriptMicroNodeCapPolicy,
    ) -> super::Mvp4TypeScriptInMemoryInventoryReport {
        let parsed = parsed(path, source);
        emit_mvp4_typescript_in_memory_first_slice_inventory_with_policy(&parsed, source, policy)
    }

    fn mvp4_local_returns_to_report(
        path: &str,
        source: &str,
    ) -> super::Mvp4TypeScriptLocalReturnsToInMemoryReport {
        let parsed = parsed(path, source);
        emit_mvp4_typescript_local_returns_to_in_memory_candidates(&parsed, source)
    }

    fn mvp4_local_returns_to_report_with_policy(
        path: &str,
        source: &str,
        policy: &Mvp4TypeScriptMicroNodeCapPolicy,
    ) -> super::Mvp4TypeScriptLocalReturnsToInMemoryReport {
        let parsed = parsed(path, source);
        emit_mvp4_typescript_local_returns_to_in_memory_candidates_with_policy(
            &parsed, source, policy,
        )
    }

    fn mvp4_local_returns_to_edge_ids(
        report: &super::Mvp4TypeScriptLocalReturnsToInMemoryReport,
    ) -> BTreeSet<String> {
        report
            .candidates
            .iter()
            .map(|candidate| candidate.micro_edge_id.clone())
            .collect()
    }

    fn mvp4_local_returns_to_edge_ids_for_function(
        path: &str,
        source: &str,
        function_name: &str,
    ) -> BTreeSet<String> {
        let inventory = mvp4_in_memory_inventory_report(path, source);
        let frames = inventory
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.node_kind == MicroNodeKind::FunctionFrame
                    && candidate.name_or_literal == function_name
            })
            .map(|candidate| candidate.micro_node_id.clone())
            .collect::<BTreeSet<_>>();
        assert!(!frames.is_empty(), "missing FunctionFrame {function_name}");

        let report = mvp4_local_returns_to_report(path, source);
        report
            .candidates
            .iter()
            .filter(|candidate| frames.contains(&candidate.tail_micro_node_id))
            .map(|candidate| candidate.micro_edge_id.clone())
            .collect()
    }

    fn assert_mvp4_local_returns_to_report_clean(
        path: &str,
        source: &str,
        report: &super::Mvp4TypeScriptLocalReturnsToInMemoryReport,
    ) {
        let inventory = mvp4_in_memory_inventory_report(path, source);
        let frames_by_id = inventory
            .candidates
            .iter()
            .filter(|candidate| candidate.node_kind == MicroNodeKind::FunctionFrame)
            .map(|candidate| (candidate.micro_node_id.clone(), candidate))
            .collect::<BTreeMap<_, _>>();
        let returns_by_id = inventory
            .candidates
            .iter()
            .filter(|candidate| candidate.node_kind == MicroNodeKind::ReturnSite)
            .map(|candidate| (candidate.micro_node_id.clone(), candidate))
            .collect::<BTreeMap<_, _>>();
        let mut ids = BTreeSet::new();

        for candidate in &report.candidates {
            assert!(
                ids.insert(candidate.micro_edge_id.clone()),
                "duplicate edge id {}",
                candidate.micro_edge_id
            );
            assert_eq!(candidate.micro_edge_kind, MicroEdgeKind::LocalReturnsTo);
            assert_eq!(candidate.repo_relative_path, path);
            assert_eq!(candidate.source_role, MicroSourceRole::Production);
            assert_eq!(candidate.exactness, MicroExactness::Exact);
            assert_eq!(
                candidate.claimability,
                "claimable_source_spanned_local_return_containment"
            );
            assert_span_inside_source(path, source, &candidate.relation_source_span);
            assert_span_inside_source(path, source, &candidate.head_source_span);
            assert_span_inside_source(path, source, &candidate.tail_source_span);
            assert_eq!(
                candidate.provenance.derivation_kind,
                MicroDerivationKind::DirectAstExtraction
            );
            assert_eq!(candidate.provenance.source_fact_ids.len(), 2);
            assert_eq!(candidate.provenance.source_spans.len(), 2);
            assert!(candidate
                .provenance
                .limitations
                .contains(&"local_ast_containment_only".to_string()));
            assert!(candidate
                .provenance
                .limitations
                .contains(&"does_not_prove_returned_value_flow".to_string()));
            assert!(candidate
                .provenance
                .limitations
                .contains(&"does_not_prove_runtime_behavior".to_string()));
            assert!(!candidate.claimability.contains("flow"));
            assert!(!candidate.claimability.contains("mutation"));
            let head = returns_by_id
                .get(&candidate.head_micro_node_id)
                .unwrap_or_else(|| {
                    panic!("missing ReturnSite head {}", candidate.head_micro_node_id)
                });
            let tail = frames_by_id
                .get(&candidate.tail_micro_node_id)
                .unwrap_or_else(|| {
                    panic!(
                        "missing FunctionFrame tail {}",
                        candidate.tail_micro_node_id
                    )
                });
            assert_eq!(head.node_kind, MicroNodeKind::ReturnSite);
            assert_eq!(tail.node_kind, MicroNodeKind::FunctionFrame);
            assert_eq!(
                head.enclosing_function_identity, tail.enclosing_function_identity,
                "LOCAL_RETURNS_TO must stay in the owning function identity"
            );
            assert_eq!(
                candidate.function_identity,
                head.enclosing_function_identity
            );
            assert_eq!(
                candidate.function_identity,
                tail.enclosing_function_identity
            );
            assert_eq!(head.repo_relative_path, tail.repo_relative_path);
            assert_eq!(candidate.repo_relative_path, head.repo_relative_path);
        }

        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.cross_function_edge_count, 0);
        assert_eq!(report.cross_file_edge_count, 0);
        assert_eq!(report.claimable_edge_missing_span_count, 0);
        assert_eq!(report.claimable_edge_missing_provenance_count, 0);
        assert_eq!(report.flow_semantic_overclaim_count, 0);
        assert_eq!(report.production_micro_edge_rows, 0);
        assert_eq!(report.local_flow_packet_rows, 0);
        assert!(!report.mutation_proof_activated);
        assert!(!report.flow_proof_activated);
    }

    fn mvp4_candidate_names(
        report: &super::Mvp4TypeScriptCoreDeclarationCandidateReport,
        kind: MicroNodeKind,
    ) -> BTreeSet<String> {
        report
            .candidates
            .iter()
            .filter(|candidate| candidate.node_kind == kind)
            .map(|candidate| candidate.name_or_literal.clone())
            .collect()
    }

    fn lexical_binding_names(scope: &super::Mvp4TypeScriptLexicalScope) -> BTreeSet<String> {
        scope
            .bindings
            .iter()
            .map(|binding| binding.name.clone())
            .collect()
    }

    #[test]
    fn mvp4_ts_scope_discovers_function_and_generator_declarations() {
        let source = "\
export function buildTotal(price: number, tax: number) {
  const subtotal = price;
  return subtotal + tax;
}

export function* ids(seed: number) {
  yield seed;
}
";
        let report = mvp4_scope_report("src/order.ts", source);

        assert!(report.eligible);
        assert_eq!(report.source_role, MicroSourceRole::Production);
        assert_eq!(report.production_micro_nodes_emitted, 0);
        assert_eq!(report.micro_edges_emitted, 0);
        assert_eq!(report.functions.len(), 2);

        let build_total = report
            .functions
            .iter()
            .find(|function| function.function_name == "buildTotal")
            .expect("buildTotal scope");
        assert_eq!(build_total.function_kind, "function_declaration");
        assert_eq!(
            lexical_binding_names(&build_total.parameter_scope),
            BTreeSet::from(["price".to_string(), "tax".to_string()])
        );
        assert!(build_total.existing_entity_link.is_some());
        assert_eq!(build_total.source_span.repo_relative_path, "src/order.ts");
        assert!(!build_total.parse_recovery);

        let ids = report
            .functions
            .iter()
            .find(|function| function.function_name == "ids")
            .expect("ids generator scope");
        assert_eq!(ids.function_kind, "generator_function_declaration");
        assert!(ids.existing_entity_link.is_some());
    }

    #[test]
    fn mvp4_ts_scope_discovers_class_methods_and_separates_same_names() {
        let source = "\
class Alpha {
  save(value: number) {
    const local = value;
    return local;
  }
}
class Beta {
  save(value: number) {
    const local = value;
    return local;
  }
}
";
        let report = mvp4_scope_report("src/services.ts", source);

        assert!(report.eligible);
        assert_eq!(report.functions.len(), 2);
        let identities = report
            .functions
            .iter()
            .map(|function| function.function_identity.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(identities.len(), 2);
        assert!(report
            .functions
            .iter()
            .any(|function| function.qualifier_path.contains(&"Alpha".to_string())));
        assert!(report
            .functions
            .iter()
            .any(|function| function.qualifier_path.contains(&"Beta".to_string())));
        assert!(report
            .functions
            .iter()
            .all(|function| function.function_kind == "method_definition"));
        assert!(report
            .functions
            .iter()
            .all(|function| function.existing_entity_link.is_some()));
    }

    #[test]
    fn mvp4_ts_scope_preserves_nested_scope_and_shadowing_identity() {
        let source = "\
export function shadow(value: number) {
  let item = value;
  if (value > 0) {
    let item = value + 1;
    {
      let item = value + 2;
    }
  }
  return item;
}
";
        let report = mvp4_scope_report("src/shadow.ts", source);
        let function = report
            .functions
            .iter()
            .find(|function| function.function_name == "shadow")
            .expect("shadow function");

        let item_scopes = function
            .lexical_scopes
            .iter()
            .filter(|scope| lexical_binding_names(scope).contains("item"))
            .map(|scope| scope.scope_path.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(item_scopes.len(), 3, "{item_scopes:?}");
        assert!(function.lexical_scopes.len() >= 3);
        assert!(function
            .lexical_scopes
            .iter()
            .all(|scope| !scope.scope_path.is_empty()));
    }

    #[test]
    fn mvp4_ts_scope_excludes_declarations_arrows_and_non_ts_surfaces() {
        let source = "\
declare function ambientOnly(value: number): void;
export function declaredOnly(value: number): number;
export function real(value: number) {
  const arrow = () => value;
  return arrow();
}
";
        let report = mvp4_scope_report("src/declarations.ts", source);
        assert!(report.eligible);
        assert_eq!(report.functions.len(), 1);
        assert_eq!(report.functions[0].function_name, "real");

        for (path, source) in [
            (
                "src/component.tsx",
                "export function Component() { return <div />; }\n",
            ),
            ("src/file.js", "export function jsOnly() { return 1; }\n"),
            (
                "src/file.jsx",
                "export function JsxOnly() { return <div />; }\n",
            ),
            ("src/types.d.ts", "export function typedOnly(): void;\n"),
        ] {
            let parsed = parsed(path, source);
            let report = discover_mvp4_typescript_scopes(&parsed, source);
            assert!(!report.eligible, "{path}");
            assert!(report.functions.is_empty(), "{path}");
        }
    }

    #[test]
    fn mvp4_ts_scope_filters_non_production_source_roles() {
        let source = "export function helper(value: number) { return value; }\n";
        for (path, role) in [
            ("tests/helper.test.ts", MicroSourceRole::Test),
            ("generated/client.ts", MicroSourceRole::Generated),
            ("src/generated/client.ts", MicroSourceRole::Generated),
            ("src/__mocks__/service.ts", MicroSourceRole::Mock),
            ("__mocks__/service.ts", MicroSourceRole::Mock),
            ("src/service.stub.ts", MicroSourceRole::Mock),
        ] {
            let report = mvp4_scope_report(path, source);
            assert!(!report.eligible, "{path}");
            assert_eq!(report.source_role, role, "{path}");
            assert!(report.functions.is_empty(), "{path}");
        }
    }

    #[test]
    fn mvp4_ts_scope_partial_parse_is_safe_and_non_claimable() {
        let source = "\
export function broken(input: number) {
  const value = ;
  return input;
}
";
        let report = mvp4_scope_report("src/broken.ts", source);

        assert!(report.eligible);
        assert_eq!(report.parser_status, "parsed_with_syntax_diagnostics");
        assert_eq!(report.functions.len(), 1);
        assert!(report.functions[0].parse_recovery);
        assert_eq!(
            report.functions[0].claimability,
            "non_claimable_recovery_scope"
        );
    }

    #[test]
    fn mvp4_ts_scope_identity_is_deterministic_and_file_distinct() {
        let source = "\
export function same(value: number) {
  const local = value;
  return local;
}
";
        let first = mvp4_scope_report("src/a.ts", source);
        let second = mvp4_scope_report("src/a.ts", source);
        let other_file = mvp4_scope_report("src/b.ts", source);

        assert_eq!(
            first.functions[0].function_identity,
            second.functions[0].function_identity
        );
        assert_eq!(
            first.functions[0].scope_path,
            second.functions[0].scope_path
        );
        assert_ne!(
            first.functions[0].function_identity,
            other_file.functions[0].function_identity
        );
    }

    #[test]
    fn mvp4_ts_scope_discovery_does_not_write_sidecar_rows() {
        let store = SqliteGraphStore::open_in_memory().expect("in-memory store");
        let source = "export function local(value: number) { return value; }\n";
        let report = mvp4_scope_report("src/local.ts", source);
        assert!(report.eligible);
        assert_eq!(report.production_micro_nodes_emitted, 0);
        assert_eq!(report.micro_edges_emitted, 0);

        let counts = store.sparse_sidecar_counts().expect("sidecar counts");
        assert_eq!(counts.get("ast_micro_nodes").copied(), Some(0));
        assert_eq!(counts.get("ast_micro_edges").copied(), Some(0));
        assert_eq!(counts.get("local_flow_packets").copied(), Some(0));
    }

    #[test]
    fn mvp4_core_declaration_candidates_emit_function_parameters_and_locals() {
        let path = "src/order.ts";
        let source = "\
export function buildTotal(price: number, tax: number) {
  const subtotal = price;
  let total = subtotal + tax;
  var legacy = total;
  return total;
}
";
        let report = mvp4_core_declaration_report(path, source);

        assert!(report.eligible);
        assert_eq!(report.production_micro_node_rows, 0);
        assert_eq!(report.micro_edge_rows, 0);
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.source_span_failures, 0);
        assert_eq!(report.binding_overclaim_count, 0);
        assert_eq!(
            mvp4_candidate_names(&report, MicroNodeKind::FunctionFrame),
            BTreeSet::from(["buildTotal".to_string()])
        );
        assert_eq!(
            mvp4_candidate_names(&report, MicroNodeKind::Parameter),
            BTreeSet::from(["price".to_string(), "tax".to_string()])
        );
        assert_eq!(
            mvp4_candidate_names(&report, MicroNodeKind::LocalBinding),
            BTreeSet::from([
                "legacy".to_string(),
                "subtotal".to_string(),
                "total".to_string(),
            ])
        );

        for candidate in &report.candidates {
            assert_span_inside_source(path, source, &candidate.source_span);
            assert_eq!(candidate.repo_relative_path, path);
            assert_eq!(candidate.language, "typescript");
            assert_eq!(candidate.frontend, "tree-sitter-typescript");
            assert_eq!(candidate.source_role, MicroSourceRole::Production);
            assert_eq!(candidate.exactness, MicroExactness::Exact);
            assert_eq!(
                candidate.provenance.derivation_kind,
                MicroDerivationKind::DirectAstExtraction
            );
            assert_eq!(candidate.provenance.source_spans.len(), 1);
            assert_eq!(
                candidate.extraction_version,
                "mvp4.1-typescript-micro-nodes-v1"
            );
            assert_eq!(candidate.row_schema_version, 1);
            assert_eq!(candidate.payload_version, 1);
            assert!(!candidate.name_or_literal.is_empty());
            assert!(
                candidate.symbol_binding_id.is_none()
                    || candidate.node_kind == MicroNodeKind::FunctionFrame
            );
        }

        let subtotal = report
            .candidates
            .iter()
            .find(|candidate| candidate.name_or_literal == "subtotal")
            .expect("subtotal local binding");
        assert_eq!(subtotal.declaration_kind.as_deref(), Some("const"));
        let total = report
            .candidates
            .iter()
            .find(|candidate| candidate.name_or_literal == "total")
            .expect("total local binding");
        assert_eq!(total.declaration_kind.as_deref(), Some("let"));
        let legacy = report
            .candidates
            .iter()
            .find(|candidate| candidate.name_or_literal == "legacy")
            .expect("legacy local binding");
        assert_eq!(legacy.declaration_kind.as_deref(), Some("var"));
    }

    #[test]
    fn mvp4_core_declaration_candidates_prevent_duplicates_and_separate_shadowing() {
        let source = "\
export function shadow(value: number) {
  let item = value;
  if (value > 0) {
    let item = value + 1;
    {
      let item = value + 2;
    }
  }
  return item;
}
";
        let report = mvp4_core_declaration_report("src/shadow.ts", source);
        let item_candidates = report
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.node_kind == MicroNodeKind::LocalBinding
                    && candidate.name_or_literal == "item"
            })
            .collect::<Vec<_>>();
        let item_ids = item_candidates
            .iter()
            .map(|candidate| candidate.micro_node_id.clone())
            .collect::<BTreeSet<_>>();
        let item_scopes = item_candidates
            .iter()
            .map(|candidate| candidate.scope_path.clone())
            .collect::<BTreeSet<_>>();

        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(item_candidates.len(), 3);
        assert_eq!(item_ids.len(), 3);
        assert_eq!(item_scopes.len(), 3);
        assert_eq!(report.binding_overclaim_count, 0);
    }

    #[test]
    fn mvp4_core_declaration_candidates_have_stable_and_intentional_rekey_ids() {
        let base = "\
export function same(value: number) {
  const local = value;
  return local;
}
";
        let whitespace = "\
export function same(value: number) {

  const local = value;
  return local;
}
";
        let comment = "\
export function same(value: number) {
  const local = value;
  // stable comment
  return local;
}
";
        let neighbor_after = "\
export function same(value: number) {
  const local = value;
  const other = value;
  return local;
}
";
        let renamed_param = "\
export function same(input: number) {
  const local = input;
  return local;
}
";
        let renamed_function = "\
export function changed(value: number) {
  const local = value;
  return local;
}
";

        let base_report = mvp4_core_declaration_report("src/a.ts", base);
        let local_id = |report: &super::Mvp4TypeScriptCoreDeclarationCandidateReport| {
            report
                .candidates
                .iter()
                .find(|candidate| {
                    candidate.node_kind == MicroNodeKind::LocalBinding
                        && candidate.name_or_literal == "local"
                })
                .expect("local candidate")
                .micro_node_id
                .clone()
        };
        let parameter_id = |report: &super::Mvp4TypeScriptCoreDeclarationCandidateReport| {
            report
                .candidates
                .iter()
                .find(|candidate| candidate.node_kind == MicroNodeKind::Parameter)
                .expect("parameter candidate")
                .micro_node_id
                .clone()
        };
        let function_id = |report: &super::Mvp4TypeScriptCoreDeclarationCandidateReport| {
            report
                .candidates
                .iter()
                .find(|candidate| candidate.node_kind == MicroNodeKind::FunctionFrame)
                .expect("function candidate")
                .micro_node_id
                .clone()
        };

        assert_eq!(
            local_id(&base_report),
            local_id(&mvp4_core_declaration_report("src/a.ts", base))
        );
        assert_eq!(
            local_id(&base_report),
            local_id(&mvp4_core_declaration_report("src/a.ts", whitespace))
        );
        assert_eq!(
            local_id(&base_report),
            local_id(&mvp4_core_declaration_report("src/a.ts", comment))
        );
        assert_eq!(
            local_id(&base_report),
            local_id(&mvp4_core_declaration_report("src/a.ts", neighbor_after))
        );
        assert_ne!(
            local_id(&base_report),
            local_id(&mvp4_core_declaration_report("src/b.ts", base))
        );
        assert_ne!(
            parameter_id(&base_report),
            parameter_id(&mvp4_core_declaration_report("src/a.ts", renamed_param))
        );
        assert_ne!(
            function_id(&base_report),
            function_id(&mvp4_core_declaration_report("src/a.ts", renamed_function))
        );
    }

    #[test]
    fn mvp4_core_declaration_candidates_mark_destructuring_unsupported_without_flattening() {
        let source = "\
export function shape({ id }: { id: number }, value: number) {
  const { local } = value as any;
  const exact = value;
  return exact;
}
";
        let report = mvp4_core_declaration_report("src/destructure.ts", source);

        assert_eq!(
            mvp4_candidate_names(&report, MicroNodeKind::Parameter),
            BTreeSet::from(["value".to_string()])
        );
        assert_eq!(
            mvp4_candidate_names(&report, MicroNodeKind::LocalBinding),
            BTreeSet::from(["exact".to_string()])
        );
        assert!(report
            .unsupported_boundaries
            .iter()
            .any(|boundary| boundary.boundary_kind == "unsupported_parameter_binding"));
        assert!(report
            .unsupported_boundaries
            .iter()
            .any(|boundary| boundary.boundary_kind == "unsupported_local_binding"));
        assert!(report
            .unsupported_boundaries
            .iter()
            .all(|boundary| boundary.exactness == MicroExactness::Unsupported));
    }

    #[test]
    fn mvp4_core_declaration_candidates_exclude_non_production_and_do_not_write_rows() {
        let source =
            "export function helper(value: number) { const local = value; return local; }\n";
        for path in [
            "tests/helper.test.ts",
            "src/generated/client.ts",
            "src/__mocks__/service.ts",
            "src/service.stub.ts",
            "src/component.tsx",
            "src/file.js",
            "src/types.d.ts",
        ] {
            let report = mvp4_core_declaration_report(path, source);
            assert!(!report.eligible, "{path}");
            assert!(report.candidates.is_empty(), "{path}");
            assert_eq!(report.production_micro_node_rows, 0, "{path}");
            assert_eq!(report.micro_edge_rows, 0, "{path}");
        }

        let store = SqliteGraphStore::open_in_memory().expect("in-memory store");
        let report = mvp4_core_declaration_report("src/local.ts", source);
        assert!(report.eligible);
        assert!(!report.candidates.is_empty());
        let counts = store.sparse_sidecar_counts().expect("sidecar counts");
        assert_eq!(counts.get("ast_micro_nodes").copied(), Some(0));
        assert_eq!(counts.get("ast_micro_edges").copied(), Some(0));
        assert_eq!(counts.get("local_flow_packets").copied(), Some(0));
    }

    #[test]
    fn mvp4_operation_site_candidates_emit_assignment_sites_without_write_flow() {
        let path = "src/assignments.ts";
        let source = "\
export function update(value: number) {
  let target = value;
  target = value;
  target += 1;
  first = second = value;
  return target;
}
";
        let report = mvp4_operation_site_report(path, source);
        let assignments = report
            .candidates
            .iter()
            .filter(|candidate| candidate.node_kind == MicroNodeKind::AssignmentSite)
            .collect::<Vec<_>>();

        assert!(report.eligible);
        assert_eq!(report.production_micro_node_rows, 0);
        assert_eq!(report.micro_edge_rows, 0);
        assert!(!report.flow_proof_activated);
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.source_span_failures, 0);
        assert_eq!(report.write_flow_overclaim_count, 0);
        assert!(
            assignments.len() >= 4,
            "expected simple, compound, and chained assignment sites: {assignments:?}"
        );
        assert!(assignments.iter().all(|candidate| {
            candidate.symbol_binding_id.is_none()
                && candidate.claimability
                    == "claimable_source_spanned_assignment_expression_existence"
                && candidate
                    .provenance
                    .limitations
                    .contains(&"does_not_claim_local_writes".to_string())
                && candidate
                    .provenance
                    .limitations
                    .contains(&"does_not_claim_value_flow".to_string())
        }));
        for candidate in assignments {
            assert_span_inside_source(path, source, &candidate.source_span);
            assert_eq!(candidate.exactness, MicroExactness::Exact);
            assert_eq!(
                candidate.provenance.derivation_kind,
                MicroDerivationKind::DirectAstExtraction
            );
            assert_eq!(
                candidate.provenance.extractor_or_adapter_version,
                "mvp4.1-typescript-operation-sites-v1"
            );
        }
    }

    #[test]
    fn mvp4_operation_site_candidates_emit_return_sites_with_nested_function_isolation() {
        let path = "src/returns.ts";
        let source = "\
export function choose(value: number) {
  if (value > 0) {
    return value;
  }
  function inner() {
    return 0;
  }
  return inner();
}
";
        let report = mvp4_operation_site_report(path, source);
        let returns = report
            .candidates
            .iter()
            .filter(|candidate| candidate.node_kind == MicroNodeKind::ReturnSite)
            .collect::<Vec<_>>();

        assert!(report.eligible);
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(returns.len(), 3, "{returns:?}");
        let outer = returns
            .iter()
            .filter(|candidate| {
                candidate
                    .scope_path
                    .iter()
                    .any(|segment| segment == "function_declaration:choose")
            })
            .count();
        let inner = returns
            .iter()
            .filter(|candidate| {
                candidate
                    .scope_path
                    .iter()
                    .any(|segment| segment == "scope:choose")
                    && candidate
                        .scope_path
                        .iter()
                        .any(|segment| segment == "function_declaration:inner")
            })
            .count();
        assert_eq!(outer, 2, "{returns:?}");
        assert_eq!(inner, 1, "{returns:?}");
        assert!(returns.iter().all(|candidate| {
            candidate.symbol_binding_id.is_none()
                && candidate.claimability == "claimable_source_spanned_return_statement_existence"
                && candidate
                    .provenance
                    .limitations
                    .contains(&"does_not_claim_returns_to_edge".to_string())
                && candidate
                    .provenance
                    .limitations
                    .contains(&"does_not_claim_complete_control_flow".to_string())
        }));
    }

    #[test]
    fn mvp4_operation_site_candidates_emit_call_sites_without_callee_targets() {
        let path = "src/calls.ts";
        let source = "\
export function invoke(value: number, service: any, registry: any, key: string, maybe: any) {
  load(value);
  service.client.send(value);
  wrap(load(value));
  registry[key](value);
  maybe?.(value);
  return service.client.send(value);
}
";
        let report = mvp4_operation_site_report(path, source);
        let calls = report
            .candidates
            .iter()
            .filter(|candidate| candidate.node_kind == MicroNodeKind::CallSite)
            .collect::<Vec<_>>();
        let call_labels = calls
            .iter()
            .map(|candidate| candidate.name_or_literal.clone())
            .collect::<BTreeSet<_>>();

        assert!(report.eligible);
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.callee_target_overclaim_count, 0);
        assert!(
            calls.len() >= 7,
            "direct, member, nested, computed, optional, and return member calls: {calls:?}"
        );
        assert!(
            call_labels.iter().any(|label| label.contains("load")),
            "{call_labels:?}"
        );
        assert!(
            call_labels
                .iter()
                .any(|label| label.contains("service.client.send")),
            "{call_labels:?}"
        );
        assert!(
            call_labels
                .iter()
                .any(|label| label.contains("registry[key]")),
            "{call_labels:?}"
        );
        assert!(calls.iter().all(|candidate| {
            candidate.symbol_binding_id.is_none()
                && candidate.claimability == "claimable_source_spanned_call_expression_existence"
                && candidate
                    .provenance
                    .limitations
                    .contains(&"does_not_claim_exact_callee_target".to_string())
                && candidate
                    .provenance
                    .limitations
                    .contains(&"does_not_claim_local_calls_edge".to_string())
        }));
        for candidate in calls {
            assert_span_inside_source(path, source, &candidate.source_span);
            assert_eq!(candidate.exactness, MicroExactness::Exact);
        }
    }

    #[test]
    fn mvp4_operation_site_candidates_are_deterministic_and_do_not_persist() {
        let path = "src/stable.ts";
        let base = "\
export function stable(value: number, service: any) {
  let target = value;
  target = value;
  service.send(value);
  return target;
}
";
        let whitespace = "\
export function stable(value: number, service: any) {

  let target = value;
  target = value;
  service.send(value);
  return target;
}
";
        let comment = "\
export function stable(value: number, service: any) {
  let target = value;
  // comment-only edit
  target = value;
  service.send(value);
  return target;
}
";
        let neighbor_after = "\
export function stable(value: number, service: any) {
  let target = value;
  target = value;
  service.send(value);
  const other = value;
  return target;
}
";
        let base_report = mvp4_operation_site_report(path, base);
        let ids_for = |report: &super::Mvp4TypeScriptOperationSiteCandidateReport| {
            report
                .candidates
                .iter()
                .map(|candidate| {
                    (
                        candidate.node_kind,
                        candidate.name_or_literal.clone(),
                        candidate.micro_node_id.clone(),
                    )
                })
                .collect::<BTreeSet<_>>()
        };

        assert_eq!(base_report.duplicate_candidate_count, 0);
        assert_eq!(base_report.source_span_failures, 0);
        assert_eq!(
            ids_for(&base_report),
            ids_for(&mvp4_operation_site_report(path, base))
        );
        assert_eq!(
            ids_for(&base_report),
            ids_for(&mvp4_operation_site_report(path, whitespace))
        );
        assert_eq!(
            ids_for(&base_report),
            ids_for(&mvp4_operation_site_report(path, comment))
        );
        assert_eq!(
            ids_for(&base_report),
            ids_for(&mvp4_operation_site_report(path, neighbor_after))
        );
        assert_ne!(
            ids_for(&base_report),
            ids_for(&mvp4_operation_site_report("src/other.ts", base))
        );

        let store = SqliteGraphStore::open_in_memory().expect("in-memory store");
        let counts = store.sparse_sidecar_counts().expect("sidecar counts");
        assert_eq!(counts.get("ast_micro_nodes").copied(), Some(0));
        assert_eq!(counts.get("ast_micro_edges").copied(), Some(0));
        assert_eq!(counts.get("local_flow_packets").copied(), Some(0));
    }

    #[test]
    fn mvp4_operation_site_candidates_filter_surfaces_and_mark_recovery() {
        let source = "\
export function broken(value: number, service: any) {
  service.send(value);
  value =
  return value;
}
";
        let report = mvp4_operation_site_report("src/broken.ts", source);
        assert!(report.eligible);
        assert_eq!(report.parser_status, "parsed_with_syntax_diagnostics");
        assert!(report
            .candidates
            .iter()
            .any(|candidate| candidate.parse_recovery
                && candidate.exactness == MicroExactness::Unknown
                && candidate.claimability == "non_claimable_recovery_candidate"));

        for path in [
            "tests/helper.test.ts",
            "src/generated/client.ts",
            "src/__mocks__/service.ts",
            "src/service.stub.ts",
            "src/component.tsx",
            "src/file.js",
            "src/types.d.ts",
        ] {
            let filtered = mvp4_operation_site_report(
                path,
                "export function helper(value: number) { helper(value); return value; }\n",
            );
            assert!(!filtered.eligible, "{path}");
            assert!(filtered.candidates.is_empty(), "{path}");
            assert_eq!(filtered.production_micro_node_rows, 0, "{path}");
            assert_eq!(filtered.micro_edge_rows, 0, "{path}");
            assert!(!filtered.flow_proof_activated, "{path}");
        }
    }

    #[test]
    fn mvp4_property_access_fixture_oracle_is_real_and_supported() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/mvp4_micro_flow_oracles/manifest.json"
        ))
        .expect("fixture manifest parses");
        let fixture = manifest["fixtures"]
            .as_array()
            .expect("fixtures array")
            .iter()
            .find(|fixture| {
                fixture["fixture_id"].as_str() == Some("ts_first_slice_function_local_micro_nodes")
            })
            .expect("first slice fixture");
        let file = &fixture["initial_source"]["files"][0];
        let path = file["path"].as_str().expect("fixture path");
        let source = file["contents"].as_str().expect("fixture source");
        assert!(source.contains("subtotal.member"));
        assert!(!source.contains("object.member"));

        let expected_property = fixture["expected_micro_nodes"]
            .as_array()
            .expect("expected micro nodes")
            .iter()
            .find(|node| node["node_kind"].as_str() == Some("PropertyAccess"))
            .expect("PropertyAccess oracle");
        assert_eq!(
            expected_property["semantic_name_or_literal"].as_str(),
            Some("subtotal.member")
        );

        let report = mvp4_property_access_report(path, source);
        let property = report
            .candidates
            .iter()
            .find(|candidate| candidate.name_or_literal == "subtotal.member")
            .expect("subtotal.member candidate");

        assert!(report.eligible);
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.source_span_failures, 0);
        assert_eq!(report.binding_overclaim_count, 0);
        assert_eq!(report.semantic_overclaim_count, 0);
        assert_eq!(property.node_kind, MicroNodeKind::PropertyAccess);
        assert_eq!(
            property.claimability,
            "claimable_source_spanned_property_access_and_static_property_token_existence"
        );
        assert_eq!(
            property.provenance.extractor_or_adapter_version,
            "mvp4.1-typescript-property-access-v1"
        );
        assert_eq!(property.symbol_binding_id, None);
        assert_eq!(property.declaration_kind.as_deref(), Some("static_member"));
        assert_eq!(property.source_span.start_line, 3);
        assert_eq!(property.source_span.start_column, Some(17));
        assert_eq!(property.source_span.end_line, 3);
        assert_eq!(property.source_span.end_column, Some(32));
    }

    #[test]
    fn mvp4_property_access_candidates_cover_direct_nested_computed_and_optional() {
        let path = "src/properties.ts";
        let source = "\
export function access(user: any, service: any, obj: any, key: string, maybe: any) {
  const direct = user.name;
  const nested = service.client.request;
  const computed = obj[key];
  const optional = maybe?.profile;
  return service.client.request;
}
";
        let report = mvp4_property_access_report(path, source);
        let labels = report
            .candidates
            .iter()
            .map(|candidate| candidate.name_or_literal.clone())
            .collect::<BTreeSet<_>>();

        assert!(report.eligible);
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.source_span_failures, 0);
        assert_eq!(report.binding_overclaim_count, 0);
        assert_eq!(report.semantic_overclaim_count, 0);
        assert!(labels.contains("user.name"), "{labels:?}");
        assert!(labels.contains("service.client.request"), "{labels:?}");
        assert!(labels.contains("obj[key]"), "{labels:?}");
        assert!(
            labels.iter().any(|label| label.contains("maybe?.profile")),
            "{labels:?}"
        );

        let computed = report
            .candidates
            .iter()
            .find(|candidate| candidate.name_or_literal == "obj[key]")
            .expect("computed member");
        assert_eq!(
            computed.claimability,
            "claimable_source_spanned_property_access_existence"
        );
        assert_eq!(
            computed.declaration_kind.as_deref(),
            Some("computed_member")
        );
        assert!(computed.provenance.limitations.contains(
            &"computed_property_name_is_unknown_unless_literal_contract_supports_it".to_string()
        ));

        let optional = report
            .candidates
            .iter()
            .find(|candidate| candidate.name_or_literal.contains("maybe?.profile"))
            .expect("optional member");
        assert_eq!(
            optional.declaration_kind.as_deref(),
            Some("optional_static_member")
        );
        assert!(optional
            .provenance
            .limitations
            .contains(&"optional_chain_does_not_imply_non_null_behavior".to_string()));

        for candidate in &report.candidates {
            assert_span_inside_source(path, source, &candidate.source_span);
            assert_eq!(candidate.node_kind, MicroNodeKind::PropertyAccess);
            assert_eq!(candidate.exactness, MicroExactness::Exact);
            assert_eq!(candidate.symbol_binding_id, None);
            assert!(candidate
                .provenance
                .limitations
                .contains(&"does_not_claim_object_type".to_string()));
            assert!(candidate.provenance.limitations.contains(
                &"does_not_claim_route_auth_sanitizer_or_security_semantics".to_string()
            ));
        }
    }

    #[test]
    fn mvp4_property_access_member_call_has_distinct_callsite_and_property_nodes() {
        let path = "src/member_call.ts";
        let source = "\
export function send(client: any, value: string) {
  client.send(value);
}
";
        let property_report = mvp4_property_access_report(path, source);
        let call_report = mvp4_operation_site_report(path, source);
        let property = property_report
            .candidates
            .iter()
            .find(|candidate| candidate.name_or_literal == "client.send")
            .expect("client.send property access");
        let call = call_report
            .candidates
            .iter()
            .find(|candidate| {
                candidate.node_kind == MicroNodeKind::CallSite
                    && candidate.name_or_literal == "call:client.send"
            })
            .expect("client.send call site");

        assert_eq!(property_report.duplicate_candidate_count, 0);
        assert_ne!(property.micro_node_id, call.micro_node_id);
        assert_ne!(property.source_span, call.source_span);
        assert_eq!(property.symbol_binding_id, None);
        assert_eq!(call.symbol_binding_id, None);
        assert_eq!(property_report.micro_edge_rows, 0);
        assert_eq!(call_report.micro_edge_rows, 0);
        assert!(!property_report.flow_proof_activated);
        assert!(!call_report.flow_proof_activated);
    }

    #[test]
    fn mvp4_property_access_candidates_are_deterministic_and_do_not_persist() {
        let path = "src/stable_property.ts";
        let base = "\
export function stable(user: any) {
  const name = user.name;
  return name;
}
";
        let whitespace = "\
export function stable(user: any) {

  const name = user.name;
  return name;
}
";
        let comment = "\
export function stable(user: any) {
  // comment-only edit
  const name = user.name;
  return name;
}
";
        let neighbor_after = "\
export function stable(user: any) {
  const name = user.name;
  const other = user.age;
  return name;
}
";
        let ids_for = |report: &super::Mvp4TypeScriptPropertyAccessCandidateReport| {
            report
                .candidates
                .iter()
                .filter(|candidate| candidate.name_or_literal == "user.name")
                .map(|candidate| candidate.micro_node_id.clone())
                .collect::<BTreeSet<_>>()
        };
        let base_report = mvp4_property_access_report(path, base);

        assert_eq!(base_report.duplicate_candidate_count, 0);
        assert_eq!(
            ids_for(&base_report),
            ids_for(&mvp4_property_access_report(path, base))
        );
        assert_eq!(
            ids_for(&base_report),
            ids_for(&mvp4_property_access_report(path, whitespace))
        );
        assert_eq!(
            ids_for(&base_report),
            ids_for(&mvp4_property_access_report(path, comment))
        );
        assert_eq!(
            ids_for(&base_report),
            ids_for(&mvp4_property_access_report(path, neighbor_after))
        );
        assert_ne!(
            ids_for(&base_report),
            ids_for(&mvp4_property_access_report("src/other.ts", base))
        );

        let store = SqliteGraphStore::open_in_memory().expect("in-memory store");
        let counts = store.sparse_sidecar_counts().expect("sidecar counts");
        assert_eq!(counts.get("ast_micro_nodes").copied(), Some(0));
        assert_eq!(counts.get("ast_micro_edges").copied(), Some(0));
        assert_eq!(counts.get("local_flow_packets").copied(), Some(0));
    }

    #[test]
    fn mvp4_property_access_candidates_filter_surfaces_without_semantic_overclaim() {
        let source = "export function helper(user: any) { return user.name; }\n";
        for path in [
            "tests/helper.test.ts",
            "src/generated/client.ts",
            "src/__mocks__/service.ts",
            "src/service.stub.ts",
            "src/component.tsx",
            "src/file.js",
            "src/types.d.ts",
        ] {
            let report = mvp4_property_access_report(path, source);
            assert!(!report.eligible, "{path}");
            assert!(report.candidates.is_empty(), "{path}");
            assert_eq!(report.production_micro_node_rows, 0, "{path}");
            assert_eq!(report.micro_edge_rows, 0, "{path}");
            assert!(!report.flow_proof_activated, "{path}");
        }

        let report = mvp4_property_access_report("src/secure.ts", source);
        assert_eq!(report.binding_overclaim_count, 0);
        assert_eq!(report.semantic_overclaim_count, 0);
        assert!(report.candidates.iter().all(|candidate| {
            candidate
                .provenance
                .limitations
                .contains(&"does_not_claim_route_auth_sanitizer_or_security_semantics".to_string())
                && candidate
                    .provenance
                    .limitations
                    .contains(&"does_not_claim_runtime_property".to_string())
        }));
    }

    #[test]
    fn mvp4_in_memory_inventory_covers_all_included_node_kinds_without_persistence() {
        let path = "src/inventory.ts";
        let source = "\
export function inventory(value: number, service: any, user: any) {
  const local = value;
  let next = local;
  next = service.compute(local);
  service.client.send(next);
  const label = user.name;
  return label;
}
";
        let report = mvp4_in_memory_inventory_report(path, source);
        let kinds = report
            .candidates
            .iter()
            .map(|candidate| candidate.node_kind)
            .collect::<BTreeSet<_>>();

        assert!(report.eligible);
        assert_eq!(report.status, "complete");
        assert_eq!(report.completeness_label, "complete");
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.stable_id_collision_count, 0);
        assert_eq!(report.claimable_node_missing_span_count, 0);
        assert_eq!(report.source_span_failures, 0);
        assert_eq!(report.binding_overclaim_count, 0);
        assert_eq!(report.semantic_overclaim_count, 0);
        assert!(kinds.contains(&MicroNodeKind::FunctionFrame), "{kinds:?}");
        assert!(kinds.contains(&MicroNodeKind::Parameter), "{kinds:?}");
        assert!(kinds.contains(&MicroNodeKind::LocalBinding), "{kinds:?}");
        assert!(kinds.contains(&MicroNodeKind::AssignmentSite), "{kinds:?}");
        assert!(kinds.contains(&MicroNodeKind::ReturnSite), "{kinds:?}");
        assert!(kinds.contains(&MicroNodeKind::CallSite), "{kinds:?}");
        assert!(kinds.contains(&MicroNodeKind::PropertyAccess), "{kinds:?}");
        assert!(report.candidates.iter().all(|candidate| {
            candidate.extraction_version == "mvp4.1-typescript-micro-nodes-v1"
                && candidate.row_schema_version == 1
                && candidate.payload_version == 1
                && candidate.language == "typescript"
                && candidate.frontend == "tree-sitter-typescript"
                && candidate.name_or_literal.len()
                    <= mvp4_typescript_first_slice_cap_policy().max_name_or_literal_bytes
        }));
        for candidate in &report.candidates {
            assert_span_inside_source(path, source, &candidate.source_span);
            assert!(!candidate.claimability.contains("flow"));
            assert!(!candidate.claimability.contains("route"));
            assert!(!candidate.claimability.contains("auth"));
        }

        let store = SqliteGraphStore::open_in_memory().expect("in-memory store");
        let counts = store.sparse_sidecar_counts().expect("sidecar counts");
        assert_eq!(counts.get("ast_micro_nodes").copied(), Some(0));
        assert_eq!(counts.get("ast_micro_edges").copied(), Some(0));
        assert_eq!(counts.get("local_flow_packets").copied(), Some(0));
        assert!(!report.production_persistence_enabled);
        assert_eq!(report.production_micro_node_rows, 0);
        assert_eq!(report.micro_edge_rows, 0);
        assert!(!report.flow_proof_activated);
    }

    #[test]
    fn mvp4_in_memory_determinism_matrix_matches_identity_contract() {
        let path = "src/determinism.ts";
        let base = "\
export function stable(value: number, service: any, user: any) {
  const local = value;
  local = service.compute(value);
  service.client.send(local);
  const label = user.name;
  return label;
}
";
        let whitespace = "\
export function stable(value: number, service: any, user: any) {

  const local = value;
  local = service.compute(value);
  service.client.send(local);
  const label = user.name;
  return label;
}
";
        let comment = "\
export function stable(value: number, service: any, user: any) {
  // comment-only edit
  const local = value;
  local = service.compute(value);
  service.client.send(local);
  const label = user.name;
  return label;
}
";
        let neighbor_before = "\
export function stable(value: number, service: any, user: any) {
  const prior = value;
  const local = value;
  local = service.compute(value);
  service.client.send(local);
  const label = user.name;
  return label;
}
";
        let neighbor_after = "\
export function stable(value: number, service: any, user: any) {
  const local = value;
  local = service.compute(value);
  service.client.send(local);
  const label = user.name;
  const after = label;
  return label;
}
";
        let unrelated_function = format!(
            "{}\n{}",
            base, "export function unrelated(input: number) { return input; }\n"
        );
        let reordered = "\
export function stable(value: number, service: any, user: any) {
  const label = user.name;
  service.client.send(label);
  const local = value;
  local = service.compute(value);
  return local;
}
";
        let parameter_renamed = base.replace("value: number", "renamed: number");
        let function_renamed = base.replace("function stable", "function renamed");

        let ids_for = |source: &str| {
            mvp4_in_memory_inventory_report(path, source)
                .candidates
                .into_iter()
                .map(|candidate| {
                    (
                        candidate.node_kind,
                        candidate.name_or_literal,
                        candidate.micro_node_id,
                    )
                })
                .collect::<BTreeSet<_>>()
        };
        let base_ids = ids_for(base);

        assert_eq!(base_ids, ids_for(base));
        assert_eq!(base_ids, ids_for(whitespace));
        assert_eq!(base_ids, ids_for(comment));
        assert!(base_ids.is_subset(&ids_for(neighbor_before)));
        assert!(base_ids.is_subset(&ids_for(neighbor_after)));
        assert!(base_ids.is_subset(&ids_for(&unrelated_function)));
        assert_ne!(base_ids, ids_for(reordered));
        assert_ne!(base_ids, ids_for(&parameter_renamed));
        assert_ne!(base_ids, ids_for(&function_renamed));
        assert_ne!(
            base_ids,
            mvp4_in_memory_inventory_report("src/renamed.ts", base)
                .candidates
                .into_iter()
                .map(|candidate| {
                    (
                        candidate.node_kind,
                        candidate.name_or_literal,
                        candidate.micro_node_id,
                    )
                })
                .collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn mvp4_in_memory_scope_shadowing_complexity_and_source_role_stress() {
        let path = "src/shadowing.ts";
        let source = "\
export async function same(value: number, service: any, obj: any, key: string) {
  const shared = value;
  let duplicated = shared;
  duplicated = service.compute(shared);
  if (duplicated) {
    const shared = obj[key];
    service.client.send(shared);
    function nested(value: number) {
      const shared = value;
      return shared;
    }
    nested(shared);
  }
  return duplicated;
}

export function* sameName(value: number) {
  const shared = value;
  yield shared;
  return shared;
}

export class Box {
  same(value: number) {
    const shared = value;
    return shared;
  }
}

export function unsupported({ destructured }: any, ...rest: any[]) {
  const { local } = destructured;
  return rest.length + local;
}
";
        let report = mvp4_in_memory_inventory_report(path, source);
        let core = mvp4_core_declaration_report(path, source);
        let shared_locals = report
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.node_kind == MicroNodeKind::LocalBinding
                    && candidate.name_or_literal == "shared"
            })
            .collect::<Vec<_>>();
        let value_parameters = report
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.node_kind == MicroNodeKind::Parameter
                    && candidate.name_or_literal == "value"
            })
            .collect::<Vec<_>>();

        assert!(report.eligible);
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.stable_id_collision_count, 0);
        assert_eq!(report.binding_overclaim_count, 0);
        assert!(shared_locals.len() >= 4, "{shared_locals:?}");
        assert!(value_parameters.len() >= 3, "{value_parameters:?}");
        assert_eq!(
            shared_locals
                .iter()
                .map(|candidate| candidate.micro_node_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            shared_locals.len()
        );
        assert_eq!(
            value_parameters
                .iter()
                .map(|candidate| candidate.micro_node_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            value_parameters.len()
        );
        assert!(core.unsupported_boundaries.iter().any(|boundary| {
            boundary.boundary_kind == "unsupported_parameter_binding"
                && boundary.claimability == "non_claimable_unsupported_binding_pattern"
                && boundary.exactness == MicroExactness::Unsupported
        }));
        assert!(core.unsupported_boundaries.iter().any(|boundary| {
            boundary.boundary_kind == "unsupported_local_binding"
                && boundary.claimability == "non_claimable_unsupported_binding_pattern"
                && boundary.exactness == MicroExactness::Unsupported
        }));
        assert!(report.candidates.iter().all(|candidate| {
            candidate.symbol_binding_id.is_none()
                || candidate.node_kind == MicroNodeKind::FunctionFrame
        }));
        assert_eq!(report.micro_edge_rows, 0);
        assert!(!report.flow_proof_activated);

        for path in [
            "tests/shadowing.test.ts",
            "src/generated/shadowing.ts",
            "src/__mocks__/shadowing.ts",
            "src/shadowing.stub.ts",
            "src/shadowing.d.ts",
            "src/shadowing.tsx",
            "src/shadowing.js",
            "src/shadowing.jsx",
        ] {
            let filtered = mvp4_in_memory_inventory_report(path, source);
            assert!(!filtered.eligible, "{path}");
            assert!(filtered.candidates.is_empty(), "{path}");
            assert_eq!(filtered.production_micro_node_rows, 0, "{path}");
            assert_eq!(filtered.micro_edge_rows, 0, "{path}");
        }
    }

    #[test]
    fn mvp4_in_memory_cap_behavior_is_explicit_and_deterministic() {
        let source = "\
export function capped(value: number, service: any, user: any) {
  const a = value;
  const b = value;
  service.one(a);
  service.two(b);
  service.three(a);
  const direct = user.name;
  return direct;
}
";
        let mut per_kind = BTreeMap::new();
        per_kind.insert(MicroNodeKind::FunctionFrame, 1);
        per_kind.insert(MicroNodeKind::Parameter, 3);
        per_kind.insert(MicroNodeKind::LocalBinding, 3);
        per_kind.insert(MicroNodeKind::AssignmentSite, 1);
        per_kind.insert(MicroNodeKind::ReturnSite, 1);
        per_kind.insert(MicroNodeKind::CallSite, 2);
        per_kind.insert(MicroNodeKind::PropertyAccess, 2);
        let below_policy = Mvp4TypeScriptMicroNodeCapPolicy {
            max_micro_nodes_per_function: 64,
            max_micro_nodes_per_file: 64,
            per_kind_nodes_per_function: per_kind.clone(),
            max_name_or_literal_bytes: 311,
            max_parser_recovery_derived_nodes: 8,
            max_omission_details_in_compact_status: 8,
        };
        let below = mvp4_in_memory_inventory_report_with_policy(
            "src/capped.ts",
            "export function below(value: number) { const a = value; return a; }\n",
            &below_policy,
        );
        assert_eq!(below.omitted_count, 0);
        assert_eq!(below.completeness_label, "complete");

        let mut exact_per_kind = per_kind.clone();
        exact_per_kind.insert(MicroNodeKind::CallSite, 3);
        exact_per_kind.insert(MicroNodeKind::PropertyAccess, 4);
        let exact_policy = Mvp4TypeScriptMicroNodeCapPolicy {
            max_micro_nodes_per_function: 17,
            max_micro_nodes_per_file: 17,
            per_kind_nodes_per_function: exact_per_kind,
            max_name_or_literal_bytes: 311,
            max_parser_recovery_derived_nodes: 8,
            max_omission_details_in_compact_status: 8,
        };
        let exact =
            mvp4_in_memory_inventory_report_with_policy("src/capped.ts", source, &exact_policy);
        assert_eq!(exact.omitted_count, 0, "{exact:?}");

        let over =
            mvp4_in_memory_inventory_report_with_policy("src/capped.ts", source, &below_policy);
        let repeat =
            mvp4_in_memory_inventory_report_with_policy("src/capped.ts", source, &below_policy);
        assert!(over.omitted_count > 0, "{over:?}");
        assert!(!over.cap_hits.is_empty(), "{over:?}");
        assert!(over
            .cap_hits
            .iter()
            .any(|hit| hit.cap_kind == "per_kind_per_function"));
        assert_eq!(over.completeness_label, "truncated_unknown_completeness");
        assert_eq!(
            over.candidates
                .iter()
                .map(|candidate| candidate.micro_node_id.clone())
                .collect::<Vec<_>>(),
            repeat
                .candidates
                .iter()
                .map(|candidate| candidate.micro_node_id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(over.omitted_count, repeat.omitted_count);
        assert_eq!(over.cap_hits, repeat.cap_hits);

        let file_policy = Mvp4TypeScriptMicroNodeCapPolicy {
            max_micro_nodes_per_function: 64,
            max_micro_nodes_per_file: 4,
            per_kind_nodes_per_function: per_kind,
            max_name_or_literal_bytes: 20,
            max_parser_recovery_derived_nodes: 8,
            max_omission_details_in_compact_status: 8,
        };
        let file_over =
            mvp4_in_memory_inventory_report_with_policy("src/capped.ts", source, &file_policy);
        assert!(file_over
            .cap_hits
            .iter()
            .any(|hit| hit.cap_kind == "file_total"));
        assert!(file_over
            .candidates
            .iter()
            .all(|candidate| candidate.name_or_literal.len() <= 20));
        assert_eq!(file_over.duplicate_candidate_count, 0);
        assert_eq!(file_over.stable_id_collision_count, 0);
    }

    #[test]
    fn mvp4_in_memory_recovery_candidates_are_downgraded_and_bounded() {
        let source = "\
export function broken(value: number, service: any) {
  const very_long_identifier_name_that_exceeds_the_test_bound_because_it_keeps_going = value;
  service.send(value);
  value =
  return value;
}
";
        let mut policy = mvp4_typescript_first_slice_cap_policy();
        policy.max_name_or_literal_bytes = 32;
        policy.max_parser_recovery_derived_nodes = 1;
        let report = mvp4_in_memory_inventory_report_with_policy("src/broken.ts", source, &policy);

        assert!(report.eligible);
        assert_eq!(report.parser_status, "parsed_with_syntax_diagnostics");
        assert!(report.candidates.iter().any(|candidate| {
            candidate.parse_recovery
                && candidate.exactness == MicroExactness::Unknown
                && candidate.claimability == "non_claimable_recovery_candidate"
        }));
        assert!(report
            .candidates
            .iter()
            .all(|candidate| candidate.name_or_literal.len() <= 32));
        assert!(report.omitted_count > 0, "{report:?}");
        assert!(report
            .cap_hits
            .iter()
            .any(|hit| hit.cap_kind == "parser_recovery"));
        assert_eq!(report.claimable_node_missing_span_count, 0);
        assert_eq!(report.binding_overclaim_count, 0);
        assert_eq!(report.micro_edge_rows, 0);
        assert!(!report.flow_proof_activated);
    }

    #[test]
    fn mvp4_local_returns_to_candidates_cover_positive_containment_cases() {
        let path = "src/local_returns.ts";
        let source = "\
export function simple(value: number) {
  return value;
}

export function bare(flag: boolean) {
  if (flag) {
    return;
  }
  return;
}

export function branches(value: number) {
  if (value > 0) {
    return value;
  } else {
    return -value;
  }
}

export function wrapped(value: number) {
  try {
    return value;
  } catch (err) {
    return 0;
  } finally {
    return value;
  }
}

export async function asyncValue(loader: any) {
  return await loader();
}

export function* generated(seed: number) {
  yield seed;
  return seed + 1;
}

class Box {
  method(value: number) {
    return value;
  }
  zero() {
    const value = 1;
  }
}
";
        let inventory = mvp4_in_memory_inventory_report(path, source);
        let retained_returns = inventory
            .candidates
            .iter()
            .filter(|candidate| candidate.node_kind == MicroNodeKind::ReturnSite)
            .count();
        let report = mvp4_local_returns_to_report(path, source);

        assert!(report.eligible);
        assert_eq!(report.status, "complete");
        assert_eq!(report.candidates.len(), retained_returns);
        assert!(retained_returns >= 10, "{retained_returns}");
        assert_eq!(report.omitted_edge_count, 0);
        assert_eq!(report.duplicate_candidate_count, 0);
        assert_eq!(report.cross_function_edge_count, 0);
        assert_eq!(report.cross_file_edge_count, 0);
        assert_eq!(report.claimable_edge_missing_span_count, 0);
        assert_eq!(report.claimable_edge_missing_provenance_count, 0);
        assert_eq!(report.flow_semantic_overclaim_count, 0);
        assert_eq!(report.production_micro_edge_rows, 0);
        assert_eq!(report.local_flow_packet_rows, 0);
        assert!(!report.mutation_proof_activated);
        assert!(!report.flow_proof_activated);

        for candidate in &report.candidates {
            assert_eq!(candidate.micro_edge_kind, MicroEdgeKind::LocalReturnsTo);
            assert_eq!(candidate.repo_relative_path, path);
            assert_eq!(candidate.language, "typescript");
            assert_eq!(candidate.frontend, "tree-sitter-typescript");
            assert_eq!(candidate.source_role, MicroSourceRole::Production);
            assert_eq!(candidate.exactness, MicroExactness::Exact);
            assert_eq!(
                candidate.claimability,
                "claimable_source_spanned_local_return_containment"
            );
            assert_eq!(candidate.row_schema_version, 1);
            assert_eq!(candidate.payload_version, 1);
            assert_eq!(
                candidate.extraction_version,
                "mvp4.2-typescript-local-returns-to-v1"
            );
            assert_span_inside_source(path, source, &candidate.relation_source_span);
            assert_span_inside_source(path, source, &candidate.head_source_span);
            assert_span_inside_source(path, source, &candidate.tail_source_span);
            assert_eq!(
                candidate.provenance.derivation_kind,
                MicroDerivationKind::DirectAstExtraction
            );
            assert_eq!(candidate.provenance.source_fact_ids.len(), 2);
            assert_eq!(candidate.provenance.source_spans.len(), 2);
            assert!(candidate
                .provenance
                .limitations
                .contains(&"local_ast_containment_only".to_string()));
            assert!(candidate
                .provenance
                .limitations
                .contains(&"does_not_prove_returned_value_flow".to_string()));
            assert!(!candidate.claimability.contains("flow_proof"));
            assert!(!candidate.claimability.contains("mutation_proof"));
        }
    }

    #[test]
    fn mvp4_local_returns_to_uses_nearest_nested_function_ownership() {
        let path = "src/nested_returns.ts";
        let source = "\
export function outer(value: number) {
  if (value > 0) {
    return value;
  }
  function inner() {
    return 0;
  }
  return inner();
}
";
        let inventory = mvp4_in_memory_inventory_report(path, source);
        let report = mvp4_local_returns_to_report(path, source);
        let inner_frame = inventory
            .candidates
            .iter()
            .find(|candidate| {
                candidate.node_kind == MicroNodeKind::FunctionFrame
                    && candidate.name_or_literal == "inner"
            })
            .expect("inner frame");
        let outer_frame = inventory
            .candidates
            .iter()
            .find(|candidate| {
                candidate.node_kind == MicroNodeKind::FunctionFrame
                    && candidate.name_or_literal == "outer"
            })
            .expect("outer frame");

        let inner_edges = report
            .candidates
            .iter()
            .filter(|candidate| candidate.tail_micro_node_id == inner_frame.micro_node_id)
            .collect::<Vec<_>>();
        let outer_edges = report
            .candidates
            .iter()
            .filter(|candidate| candidate.tail_micro_node_id == outer_frame.micro_node_id)
            .collect::<Vec<_>>();

        assert_eq!(report.candidates.len(), 3, "{report:?}");
        assert_eq!(inner_edges.len(), 1, "{inner_edges:?}");
        assert_eq!(outer_edges.len(), 2, "{outer_edges:?}");
        assert_ne!(
            inner_edges[0].tail_micro_node_id, outer_frame.micro_node_id,
            "nested return must not link to outer FunctionFrame"
        );
        assert_eq!(report.cross_function_edge_count, 0);
    }

    #[test]
    fn mvp4_local_returns_to_negative_semantics_and_source_boundaries_emit_no_edges() {
        let source = "\
export function textOnly() {
  const message = \"return value\";
  const returnLike = { returnValue: message };
  // return message;
  returnLike.returnValue;
}
";
        let report = mvp4_local_returns_to_report("src/text_only.ts", source);
        assert!(report.eligible);
        assert!(report.candidates.is_empty(), "{report:?}");
        assert_eq!(report.production_micro_edge_rows, 0);
        assert_eq!(report.flow_semantic_overclaim_count, 0);

        for path in [
            "tests/helper.test.ts",
            "src/generated/client.ts",
            "src/__mocks__/service.ts",
            "src/service.stub.ts",
            "src/component.tsx",
            "src/file.js",
            "src/file.jsx",
            "src/types.d.ts",
        ] {
            let filtered =
                mvp4_local_returns_to_report(path, "export function helper() { return 1; }\n");
            assert!(!filtered.eligible, "{path}");
            assert!(filtered.candidates.is_empty(), "{path}");
            assert_eq!(filtered.production_micro_edge_rows, 0, "{path}");
            assert_eq!(filtered.local_flow_packet_rows, 0, "{path}");
            assert!(!filtered.flow_proof_activated, "{path}");
        }
    }

    #[test]
    fn mvp4_local_returns_to_parser_recovery_is_diagnostic_not_exact_edge() {
        let source = "\
export function broken(value: number, service: any) {
  service.send(value);
  value =
  return value;
}
";
        let report = mvp4_local_returns_to_report("src/broken_returns.ts", source);

        assert!(report.eligible);
        assert_eq!(report.parser_status, "parsed_with_syntax_diagnostics");
        assert!(report.candidates.is_empty(), "{report:?}");
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.diagnostic_kind.starts_with("parser_recovery_")
                    && diagnostic.classification == "unsupported_degraded_condition"
                    && diagnostic.recommended_action_kind == "unsupported_or_unknown"
            }),
            "{report:#?}"
        );
        assert_eq!(report.production_micro_edge_rows, 0);
        assert!(!report.flow_proof_activated);
    }

    #[test]
    fn mvp4_local_returns_to_cap_omissions_are_explicit_and_non_dangling() {
        let source = "\
export function capped(value: number) {
  if (value > 0) {
    return value;
  }
  return 0;
}
";
        let mut policy = mvp4_typescript_first_slice_cap_policy();
        policy
            .per_kind_nodes_per_function
            .insert(MicroNodeKind::ReturnSite, 1);
        let report =
            mvp4_local_returns_to_report_with_policy("src/capped_returns.ts", source, &policy);
        assert_eq!(report.candidates.len(), 1, "{report:?}");
        assert!(report.omitted_edge_count > 0, "{report:?}");
        assert_eq!(
            report.completeness_label,
            "truncated_or_degraded_unknown_completeness"
        );
        assert!(report.candidates[0].cap_omission_state.is_some());
        assert_eq!(report.cross_function_edge_count, 0);
        assert_eq!(report.cross_file_edge_count, 0);

        let mut missing_frame_policy = mvp4_typescript_first_slice_cap_policy();
        missing_frame_policy
            .per_kind_nodes_per_function
            .insert(MicroNodeKind::FunctionFrame, 0);
        let missing_frame = mvp4_local_returns_to_report_with_policy(
            "src/missing_frame.ts",
            source,
            &missing_frame_policy,
        );
        assert!(missing_frame.candidates.is_empty(), "{missing_frame:?}");
        assert!(missing_frame.omitted_edge_count > 0, "{missing_frame:?}");
        assert!(missing_frame.diagnostics.iter().any(|diagnostic| {
            diagnostic.diagnostic_kind == "function_frame_endpoint_omitted_by_cap"
                && diagnostic.recommended_action_kind == "unsupported_or_unknown"
        }));
    }

    #[test]
    fn mvp4_local_returns_to_is_deterministic_deduped_and_does_not_persist() {
        let path = "src/stable_returns.ts";
        let source = "\
export function stable(value: number) {
  if (value) {
    return value;
  }
  return 0;
}
";
        let first = mvp4_local_returns_to_report(path, source);
        let second = mvp4_local_returns_to_report(path, source);
        let first_ids = first
            .candidates
            .iter()
            .map(|candidate| candidate.micro_edge_id.clone())
            .collect::<Vec<_>>();
        let second_ids = second
            .candidates
            .iter()
            .map(|candidate| candidate.micro_edge_id.clone())
            .collect::<Vec<_>>();

        assert_eq!(first_ids, second_ids);
        assert_eq!(first.duplicate_candidate_count, 0);
        assert_eq!(
            first_ids.iter().collect::<BTreeSet<_>>().len(),
            first_ids.len()
        );
        assert_eq!(first.cross_function_edge_count, 0);
        assert_eq!(first.cross_file_edge_count, 0);
        assert_eq!(first.claimable_edge_missing_span_count, 0);
        assert_eq!(first.claimable_edge_missing_provenance_count, 0);
        assert_eq!(first.flow_semantic_overclaim_count, 0);

        let store = SqliteGraphStore::open_in_memory().expect("in-memory store");
        let counts = store.sparse_sidecar_counts().expect("sidecar counts");
        assert_eq!(counts.get("ast_micro_nodes").copied(), Some(0));
        assert_eq!(counts.get("ast_micro_edges").copied(), Some(0));
        assert_eq!(counts.get("local_flow_packets").copied(), Some(0));
    }

    #[test]
    fn mvp4_local_returns_to_identity_stability_matrix_matches_contract() {
        let path = "src/identity_matrix.ts";
        let base = "\
export function stable(value: number) {
  return value;
}
";
        let whitespace_only = "\
export function stable(value: number) {

    return value;
}
";
        let comment_only = "\
export function stable(value: number) {
  // harmless comment
  return value;
}
";
        let statement_before = "\
export function stable(value: number) {
  const marker = value + 1;
  return value;
}
";
        let statement_after = "\
export function stable(value: number) {
  return value;
  const marker = value + 1;
}
";
        let unrelated_function_before = "\
export function helper() {
  return 0;
}

export function stable(value: number) {
  return value;
}
";
        let moved_unique_return = "\
export function stable(value: number) {
  if (value > 0) {
    return value + 1;
  }
}
";
        let moved_to_nested = "\
export function stable(value: number) {
  function nested() {
    return value;
  }
  return nested();
}
";
        let renamed_function = "\
export function renamed(value: number) {
  return value;
}
";

        let base_ids = mvp4_local_returns_to_edge_ids_for_function(path, base, "stable");
        assert_eq!(base_ids.len(), 1);
        assert_eq!(
            base_ids,
            mvp4_local_returns_to_edge_ids_for_function(path, whitespace_only, "stable")
        );
        assert_eq!(
            base_ids,
            mvp4_local_returns_to_edge_ids_for_function(path, comment_only, "stable")
        );
        assert_eq!(
            base_ids,
            mvp4_local_returns_to_edge_ids_for_function(path, statement_before, "stable")
        );
        assert_eq!(
            base_ids,
            mvp4_local_returns_to_edge_ids_for_function(path, statement_after, "stable")
        );

        let inserted_report = mvp4_local_returns_to_report(path, unrelated_function_before);
        assert_mvp4_local_returns_to_report_clean(
            path,
            unrelated_function_before,
            &inserted_report,
        );
        assert_eq!(inserted_report.candidates.len(), 2);

        let moved_ids =
            mvp4_local_returns_to_edge_ids_for_function(path, moved_unique_return, "stable");
        assert_eq!(moved_ids.len(), 1);
        assert_ne!(base_ids, moved_ids);

        let nested_report = mvp4_local_returns_to_report(path, moved_to_nested);
        assert_mvp4_local_returns_to_report_clean(path, moved_to_nested, &nested_report);
        assert_eq!(nested_report.candidates.len(), 2);
        assert_ne!(
            base_ids,
            mvp4_local_returns_to_edge_ids_for_function(path, moved_to_nested, "nested")
        );

        assert_ne!(
            base_ids,
            mvp4_local_returns_to_edge_ids_for_function(path, renamed_function, "renamed")
        );
        assert_ne!(
            base_ids,
            mvp4_local_returns_to_edge_ids_for_function(
                "src/identity_matrix_renamed.ts",
                base,
                "stable"
            )
        );

        let duplicate_path_ids = mvp4_local_returns_to_edge_ids_for_function(
            "src/copy/identity_matrix.ts",
            base,
            "stable",
        );
        assert!(base_ids.is_disjoint(&duplicate_path_ids));

        let same_return_text = "\
class A {
  run(value: number) {
    return value;
  }
}

class B {
  run(value: number) {
    return value;
  }
}

export function run(value: number) {
  return value;
}
";
        let report = mvp4_local_returns_to_report("src/same_text.ts", same_return_text);
        assert_mvp4_local_returns_to_report_clean("src/same_text.ts", same_return_text, &report);
        assert_eq!(report.candidates.len(), 3);
        assert_eq!(
            mvp4_local_returns_to_edge_ids(&report).len(),
            report.candidates.len()
        );
    }

    #[test]
    fn mvp4_local_returns_to_ownership_adversaries_stay_in_nearest_function() {
        let path = "src/ownership_adversaries.ts";
        let source = "\
export function outer(value: number) {
  if (value > 0) {
    return value;
  }
  function middle(input: number) {
    function inner() {
      return input;
    }
    switch (input) {
      case 1:
        break;
      default:
        break;
    }
    try {
      return input + 1;
    } catch (err) {
      return 0;
    } finally {
      return input;
    }
  }
  throw new Error('stop');
  return middle(value);
}

class Wrapper {
  method(value: number) {
    function insideMethod() {
      return value;
    }
    return insideMethod();
  }
}
";
        let inventory = mvp4_in_memory_inventory_report(path, source);
        let report = mvp4_local_returns_to_report(path, source);
        assert_mvp4_local_returns_to_report_clean(path, source, &report);

        let frames = inventory
            .candidates
            .iter()
            .filter(|candidate| candidate.node_kind == MicroNodeKind::FunctionFrame)
            .map(|candidate| {
                (
                    candidate.name_or_literal.clone(),
                    candidate.micro_node_id.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let edge_count_for = |name: &str| {
            let frame_id = frames
                .get(name)
                .unwrap_or_else(|| panic!("missing frame {name}"));
            report
                .candidates
                .iter()
                .filter(|candidate| candidate.tail_micro_node_id == *frame_id)
                .count()
        };

        assert_eq!(edge_count_for("outer"), 2);
        assert_eq!(edge_count_for("middle"), 3);
        assert_eq!(edge_count_for("inner"), 1);
        assert_eq!(edge_count_for("method"), 1);
        assert_eq!(edge_count_for("insideMethod"), 1);
        assert!(frames.keys().all(|name| name != "Wrapper"));
    }

    #[test]
    fn mvp4_local_returns_to_exactness_adversaries_do_not_fallback_to_heuristics() {
        let unsupported_arrow = "\
export function holder(value: number) {
  const callback = () => {
    return value;
  };
  return value;
}
";
        let report = mvp4_local_returns_to_report("src/unsupported_arrow.ts", unsupported_arrow);
        assert_mvp4_local_returns_to_report_clean(
            "src/unsupported_arrow.ts",
            unsupported_arrow,
            &report,
        );
        assert_eq!(
            report.candidates.len(),
            1,
            "unsupported arrow return must not leak into the outer function"
        );

        let malformed_boundary = "\
export function broken(value: number) {
  if (value > 0) {
    return value;
";
        let broken = mvp4_local_returns_to_report("src/broken_boundary.ts", malformed_boundary);
        assert!(broken.eligible);
        assert_eq!(broken.parser_status, "parsed_with_syntax_diagnostics");
        assert!(broken.candidates.is_empty(), "{broken:?}");
        assert!(broken.diagnostics.iter().any(|diagnostic| {
            diagnostic.classification == "unsupported_degraded_condition"
                && diagnostic
                    .missing_requirements
                    .iter()
                    .any(|requirement| requirement == "no_parser_recovery_ambiguity")
        }));

        for path in [
            "src/unknown-source.tsx",
            "src/unknown-source.js",
            "src/unknown-source.jsx",
            "src/unknown-source.d.ts",
            "src/__mocks__/unknown-source.ts",
            "src/generated/unknown-source.ts",
            "tests/unknown-source.test.ts",
        ] {
            let filtered = mvp4_local_returns_to_report(path, "export function f() { return 1; }");
            assert!(!filtered.eligible, "{path}");
            assert!(filtered.candidates.is_empty(), "{path}");
            assert_eq!(filtered.production_micro_edge_rows, 0, "{path}");
        }

        let capped_source = "\
export function capped(value: number) {
  return value;
}
";
        let mut stale_endpoint_policy = mvp4_typescript_first_slice_cap_policy();
        stale_endpoint_policy
            .per_kind_nodes_per_function
            .insert(MicroNodeKind::FunctionFrame, 0);
        let capped = mvp4_local_returns_to_report_with_policy(
            "src/capped_endpoint.ts",
            capped_source,
            &stale_endpoint_policy,
        );
        assert!(capped.candidates.is_empty(), "{capped:?}");
        assert!(capped.omitted_edge_count > 0, "{capped:?}");
        assert!(capped.diagnostics.iter().all(|diagnostic| {
            diagnostic.recommended_action_kind == "unsupported_or_unknown"
                || diagnostic.recommended_action_kind == "reindex_or_repair"
        }));
    }

    #[test]
    fn mvp4_local_returns_to_cap_stress_is_deterministic_and_explicit() {
        fn source_with_returns(return_count: usize) -> String {
            let mut source = String::from("export function many(value: number) {\n");
            for index in 0..return_count {
                source.push_str(&format!("  if (value === {index}) {{ return {index}; }}\n"));
            }
            source.push_str("}\n");
            source
        }

        let mut policy = mvp4_typescript_first_slice_cap_policy();
        policy
            .per_kind_nodes_per_function
            .insert(MicroNodeKind::ReturnSite, 3);

        let below = source_with_returns(2);
        let below_report =
            mvp4_local_returns_to_report_with_policy("src/return_cap_below.ts", &below, &policy);
        assert_mvp4_local_returns_to_report_clean("src/return_cap_below.ts", &below, &below_report);
        assert_eq!(below_report.candidates.len(), 2);
        assert_eq!(below_report.omitted_edge_count, 0);

        let at = source_with_returns(3);
        let at_report =
            mvp4_local_returns_to_report_with_policy("src/return_cap_at.ts", &at, &policy);
        assert_mvp4_local_returns_to_report_clean("src/return_cap_at.ts", &at, &at_report);
        assert_eq!(at_report.candidates.len(), 3);
        assert_eq!(at_report.omitted_edge_count, 0);

        let over = source_with_returns(8);
        let first_over =
            mvp4_local_returns_to_report_with_policy("src/return_cap_over.ts", &over, &policy);
        let second_over =
            mvp4_local_returns_to_report_with_policy("src/return_cap_over.ts", &over, &policy);
        assert_mvp4_local_returns_to_report_clean("src/return_cap_over.ts", &over, &first_over);
        assert_eq!(first_over.candidates.len(), 3);
        assert!(first_over.omitted_edge_count >= 5, "{first_over:?}");
        assert_eq!(
            first_over.completeness_label,
            "truncated_or_degraded_unknown_completeness"
        );
        assert!(first_over
            .candidates
            .iter()
            .all(|candidate| candidate.cap_omission_state.is_some()));
        assert_eq!(
            mvp4_local_returns_to_edge_ids(&first_over),
            mvp4_local_returns_to_edge_ids(&second_over)
        );
        assert_eq!(
            first_over.omitted_edge_count,
            second_over.omitted_edge_count
        );

        let mut file_cap_policy = policy.clone();
        file_cap_policy.max_micro_nodes_per_file = 2;
        let file_capped = mvp4_local_returns_to_report_with_policy(
            "src/return_file_cap.ts",
            &source_with_returns(5),
            &file_cap_policy,
        );
        assert!(file_capped.candidates.len() <= 1, "{file_capped:?}");
        assert!(file_capped.omitted_edge_count > 0, "{file_capped:?}");
        assert_eq!(
            file_capped.completeness_label,
            "truncated_or_degraded_unknown_completeness"
        );
    }

    #[test]
    fn mvp4_local_returns_to_proof_invariants_hold_for_adversarial_inventory() {
        let path = "src/proof_invariants.ts";
        let source = "\
export async function proof(loader: any, value: number) {
  const result = await loader(value);
  if (result) {
    return result;
  }
  return value;
}
";
        let report = mvp4_local_returns_to_report(path, source);
        assert_mvp4_local_returns_to_report_clean(path, source, &report);
        assert_eq!(report.candidates.len(), 2);
        assert!(report.diagnostics.is_empty(), "{report:?}");
        let serialized = serde_json::to_string(&report.candidates).expect("serialize edges");
        assert!(!serialized.contains("export async function proof"));
        assert!(!serialized.contains("await loader"));
        assert!(!serialized.contains("route"));
        assert!(!serialized.contains("auth"));
        assert!(!serialized.contains("security"));
        assert_eq!(report.local_flow_packet_rows, 0);
        assert!(!report.flow_proof_activated);
        assert!(!report.mutation_proof_activated);
    }

    fn import_entity<'a>(
        extraction: &'a super::BasicExtraction,
        local_name: &str,
        import_kind: &str,
    ) -> &'a Entity {
        extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Import
                    && metadata_str(entity, "local_name") == Some(local_name)
                    && metadata_str(entity, "import_kind") == Some(import_kind)
            })
            .unwrap_or_else(|| panic!("missing import entity {import_kind}:{local_name}"))
    }

    #[test]
    fn detects_supported_file_types() {
        assert_eq!(
            detect_language("src/app.js"),
            Some(SourceLanguage::JavaScript)
        );
        assert_eq!(detect_language("src/app.jsx"), Some(SourceLanguage::Jsx));
        assert_eq!(
            detect_language("src/app.ts"),
            Some(SourceLanguage::TypeScript)
        );
        assert_eq!(detect_language("src/app.tsx"), Some(SourceLanguage::Tsx));
        assert_eq!(detect_language("src/app.py"), Some(SourceLanguage::Python));
        assert_eq!(detect_language("src/app.go"), Some(SourceLanguage::Go));
        assert_eq!(detect_language("src/app.rs"), Some(SourceLanguage::Rust));
        assert_eq!(detect_language("src/App.java"), Some(SourceLanguage::Java));
        assert_eq!(detect_language("src/App.cs"), Some(SourceLanguage::CSharp));
        assert_eq!(detect_language("src/app.c"), Some(SourceLanguage::C));
        assert_eq!(detect_language("src/app.cpp"), Some(SourceLanguage::Cpp));
        assert_eq!(detect_language("src/app.rb"), Some(SourceLanguage::Ruby));
        assert_eq!(detect_language("src/app.php"), Some(SourceLanguage::Php));
        assert_eq!(
            detect_language("src/MIXED_CASE_APP.TS"),
            Some(SourceLanguage::TypeScript)
        );
        assert_eq!(
            detect_language("src/MixedCase.PY"),
            Some(SourceLanguage::Python)
        );
        assert_eq!(detect_language("src/README.md"), None);
    }

    #[test]
    fn detects_supported_no_extension_shebangs_from_source_without_promoting_unknown_extensions() {
        assert_eq!(
            detect_language_with_source("scripts/run-python", "#!/usr/bin/env python3\nprint(1)\n"),
            Some(SourceLanguage::Python)
        );
        assert_eq!(
            detect_language_with_source("scripts/run-js", "#!/usr/bin/env node\nconsole.log(1)\n"),
            Some(SourceLanguage::JavaScript)
        );
        assert_eq!(
            detect_language_with_source(
                "scripts/run-ts",
                "#!/usr/bin/env ts-node\nconsole.log(1)\n"
            ),
            Some(SourceLanguage::TypeScript)
        );
        assert_eq!(
            detect_language_with_source("scripts/run-ruby", "#!/usr/bin/env ruby\nputs 1\n"),
            Some(SourceLanguage::Ruby)
        );
        assert_eq!(
            detect_language_with_source("scripts/run-php", "#!/usr/bin/env php\n<?php echo 1;\n"),
            Some(SourceLanguage::Php)
        );
        assert_eq!(
            detect_language_with_source("scripts/run-shell", "#!/bin/sh\necho nope\n"),
            None
        );
        assert_eq!(
            detect_language_with_source("docs/readme.md", "#!/usr/bin/env python3\nprint(1)\n"),
            None
        );

        let parsed = parsed(
            "scripts/no-extension-python",
            "#!/usr/bin/env python3\n\ndef run():\n    return 1\n",
        );
        assert_eq!(parsed.language, SourceLanguage::Python);
        assert!(!parsed.has_syntax_errors(), "{:?}", parsed.diagnostics);
    }

    #[test]
    fn registry_lists_every_post_mvp_language_with_honest_tiers() {
        let registry = super::default_frontend_registry();
        let languages = registry
            .frontends()
            .iter()
            .map(|frontend| frontend.info().language_id)
            .collect::<BTreeSet<_>>();

        for expected in [
            "javascript",
            "jsx",
            "typescript",
            "tsx",
            "python",
            "go",
            "rust",
            "java",
            "csharp",
            "c",
            "cpp",
            "ruby",
            "php",
        ] {
            assert!(languages.contains(expected), "missing {expected}");
        }

        let python = registry
            .info_for_language(SourceLanguage::Python)
            .expect("python frontend");
        assert_eq!(python.support_tier.number(), 3);
        assert!(python.tree_sitter_grammar_available);
        assert!(!python.compiler_resolver_available);
        assert!(python
            .known_limitations
            .iter()
            .any(|limitation| limitation.contains("dataflow")));

        let java = registry
            .info_for_language(SourceLanguage::Java)
            .expect("java frontend");
        assert_eq!(java.support_tier.number(), 1);
        assert!(java
            .known_limitations
            .iter()
            .any(|limitation| limitation.contains("syntax/entity")));
    }

    #[test]
    fn unsupported_file_types_are_skipped_cleanly() {
        let result = parser().parse("README.md", "# nope");

        match result {
            Ok(None) => {}
            other => panic!("expected unsupported file to return Ok(None), got {other:?}"),
        }
    }

    #[test]
    fn parses_js_fixture() {
        let parsed = parsed("fixtures/basic.js", JS_FIXTURE);

        assert_eq!(parsed.language, SourceLanguage::JavaScript);
        assert_eq!(parsed.root_node.kind, "program");
        assert!(!parsed.has_syntax_errors(), "{:?}", parsed.diagnostics);
        assert!(parsed.root_node.named_child_count >= 2);
    }

    #[test]
    fn parses_ts_fixture() {
        let parsed = parsed("fixtures/basic.ts", TS_FIXTURE);

        assert_eq!(parsed.language, SourceLanguage::TypeScript);
        assert_eq!(parsed.root_node.kind, "program");
        assert!(!parsed.has_syntax_errors(), "{:?}", parsed.diagnostics);
        assert!(parsed.root_node.named_child_count >= 3);
    }

    #[test]
    fn parses_tsx_fixture() {
        let parsed = parsed("fixtures/component.tsx", TSX_FIXTURE);

        assert_eq!(parsed.language, SourceLanguage::Tsx);
        assert_eq!(parsed.root_node.kind, "program");
        assert!(!parsed.has_syntax_errors(), "{:?}", parsed.diagnostics);
        assert!(parsed.root_node.named_child_count >= 2);
    }

    #[test]
    fn python_go_and_rust_extract_entities_imports_and_honest_calls() {
        let cases = [
            (
                "fixtures/sample.py",
                "import os\nfrom pkg.service import make\n\nclass Greeter:\n    def __init__(self, name):\n        self.name = name\n\n    def greet(self, msg):\n        local = msg\n        return local\n\ndef helper(value):\n    local = value\n    return local\n",
                SourceLanguage::Python,
                &[EntityKind::Class, EntityKind::Method, EntityKind::Function, EntityKind::Import][..],
            ),
            (
                "fixtures/sample.go",
                "package service\n\nimport \"fmt\"\n\ntype Greeter struct { Name string }\n\nfunc NewGreeter(name string) Greeter {\n    local := name\n    return Greeter{Name: local}\n}\n\nfunc (g Greeter) Greet(msg string) string {\n    return fmt.Sprint(g.Name, msg)\n}\n",
                SourceLanguage::Go,
                &[EntityKind::Class, EntityKind::Function, EntityKind::Method, EntityKind::Import][..],
            ),
            (
                "fixtures/sample.rs",
                "use std::fmt;\n\npub struct Greeter { name: String }\npub trait Speak { fn speak(&self) -> String; }\npub fn build(name: String) -> Greeter {\n    let local = name;\n    Greeter { name: local }\n}\n",
                SourceLanguage::Rust,
                &[EntityKind::Class, EntityKind::Trait, EntityKind::Function, EntityKind::Import][..],
            ),
        ];

        for (path, source, language, expected_kinds) in cases {
            let parsed = parsed(path, source);
            assert_eq!(parsed.language, language);
            let extraction = extract_basic_entities(&parsed, source);
            let kinds = extraction
                .entities
                .iter()
                .map(|entity| entity.kind)
                .collect::<BTreeSet<_>>();
            for expected in expected_kinds {
                assert!(kinds.contains(expected), "{path} missing {expected:?}");
            }
            assert!(extraction
                .edges
                .iter()
                .any(|edge| edge.relation == RelationKind::Imports));
            assert!(
                extraction.edges.iter().all(|edge| {
                    edge.exactness == Exactness::ParserVerified
                    || (edge.relation == RelationKind::Calls
                        && edge.exactness == Exactness::StaticHeuristic)
                    || (edge.relation == RelationKind::Callee
                        && edge.exactness == Exactness::StaticHeuristic)
                    || (matches!(
                        edge.relation,
                        RelationKind::Reads
                            | RelationKind::Writes
                            | RelationKind::Mutates
                            | RelationKind::AssignedFrom
                            | RelationKind::FlowsTo
                    ) && edge.exactness == Exactness::StaticHeuristic)
                    // Unresolved import TARGETS (e.g. `from pkg.service import
                    // make`, `use std::fmt`, `import "fmt"`) enter the Q4
                    // unresolved-reference lane as honestly-labeled
                    // StaticHeuristic IMPORTS edges: the parser cannot resolve
                    // cross-module/stdlib targets, so the target is classified
                    // downstream (builtin_or_std / external_dependency / ...).
                    || (edge.relation == RelationKind::Imports
                        && edge.exactness == Exactness::StaticHeuristic)
                }),
                "{path}: dishonest edge exactness: {:?}",
                extraction.edges
            );
        }
    }

    #[test]
    fn python_go_and_rust_extract_conservative_call_edges() {
        let cases = [
            (
                "fixtures/calls.py",
                "def helper(value):\n    return value\n\ndef run(value):\n    return helper(value)\n",
                "helper",
            ),
            (
                "fixtures/calls.go",
                "package main\nfunc helper(value string) string { return value }\nfunc run(value string) string { return helper(value) }\n",
                "helper",
            ),
            (
                "fixtures/calls.rs",
                "pub fn helper(value: String) -> String { value }\npub fn run(value: String) -> String { helper(value) }\n",
                "helper",
            ),
        ];

        for (path, source, callee) in cases {
            let extraction = extraction(path, source);
            let callee_entity = extraction
                .entities
                .iter()
                .find(|entity| entity.name == callee && entity.kind == EntityKind::Function)
                .unwrap_or_else(|| panic!("{path} missing callee entity"));
            assert!(extraction.edges.iter().any(|edge| {
                edge.relation == RelationKind::Calls
                    && edge.tail_id == callee_entity.id
                    && edge.exactness == Exactness::ParserVerified
            }));
            assert!(extraction
                .edges
                .iter()
                .any(|edge| edge.relation == RelationKind::Callee));
            assert!(extraction
                .entities
                .iter()
                .any(|entity| entity.kind == EntityKind::CallSite));
        }
    }

    #[test]
    fn rust_later_sibling_calls_are_exact_after_predeclaration() {
        let source = "pub fn run(value: String) -> String { helper(value) }\n\npub fn helper(value: String) -> String { value }\n";
        let extraction = extraction("fixtures/later_sibling.rs", source);
        let helper = extraction
            .entities
            .iter()
            .find(|entity| entity.name == "helper" && entity.kind == EntityKind::Function)
            .expect("helper entity");

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Calls
                && edge.tail_id == helper.id
                && edge.exactness == Exactness::ParserVerified
        }));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Callee
                && edge.tail_id == helper.id
                && edge.exactness == Exactness::ParserVerified
        }));
    }

    #[test]
    fn rust_method_calls_are_exact_when_receiver_type_is_resolved() {
        let unique = "struct Worker;\nimpl Worker {\n    fn run(&self) { self.helper(); }\n    fn helper(&self) {}\n}\n";
        let unique_extraction = extraction("fixtures/rust_unique_method.rs", unique);
        let unique_helper = unique_extraction
            .entities
            .iter()
            .find(|entity| entity.name == "helper" && entity.kind == EntityKind::Method)
            .expect("unique helper method");
        assert!(unique_extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Calls
                && edge.tail_id == unique_helper.id
                && edge.exactness == Exactness::ParserVerified
        }));

        let duplicate_names = "struct A;\nstruct B;\nimpl A {\n    fn run(&self) { self.helper(); }\n    fn helper(&self) {}\n}\nimpl B {\n    fn helper(&self) {}\n}\n";
        let duplicate_extraction = extraction("fixtures/rust_duplicate_method.rs", duplicate_names);
        let run = duplicate_extraction
            .entities
            .iter()
            .find(|entity| entity.name == "run" && entity.kind == EntityKind::Method)
            .expect("A::run");
        let a_helper = duplicate_extraction
            .entities
            .iter()
            .find(|entity| {
                entity.name == "helper"
                    && entity.kind == EntityKind::Method
                    && entity
                        .metadata
                        .get("rust_impl_type")
                        .and_then(serde_json::Value::as_str)
                        == Some("A")
            })
            .expect("A::helper");
        let b_helper = duplicate_extraction
            .entities
            .iter()
            .find(|entity| {
                entity.name == "helper"
                    && entity.kind == EntityKind::Method
                    && entity
                        .metadata
                        .get("rust_impl_type")
                        .and_then(serde_json::Value::as_str)
                        == Some("B")
            })
            .expect("B::helper");
        let calls = duplicate_extraction
            .edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Calls && edge.head_id == run.id)
            .collect::<Vec<_>>();
        assert!(calls.iter().any(|edge| {
            edge.tail_id == a_helper.id && edge.exactness == Exactness::ParserVerified
        }));
        assert!(
            calls.iter().all(|edge| edge.tail_id != b_helper.id),
            "receiver-typed self.helper must not target another type's helper"
        );
    }

    #[test]
    fn rust_deterministic_path_alias_self_and_local_method_calls_are_exact() {
        let source = r#"
pub fn helper() {}

pub fn direct_caller() {
    helper();
}

pub mod util {
    pub fn helper() {}

    pub fn self_caller() {
        self::helper();
    }

    pub mod nested {
        pub fn child() {
            super::helper();
        }
    }
}

use crate::util::helper as h;

pub fn module_caller() {
    util::helper();
    crate::util::helper();
    h();
}

pub struct Service;

impl Service {
    pub fn new() -> Self { Service }
    pub fn build() {
        Self::new();
        Service::new();
    }
    pub fn run(&self) {}
}

pub fn method_caller() {
    let s = Service::new();
    s.run();
}
"#;
        let extraction = extraction("src/lib.rs", source);
        let entity = |name: &str, kind: EntityKind, qname_suffix: &str| {
            extraction
                .entities
                .iter()
                .find(|entity| {
                    entity.name == name
                        && entity.kind == kind
                        && entity.qualified_name.ends_with(qname_suffix)
                })
                .unwrap_or_else(|| panic!("missing {kind:?} {name} ending {qname_suffix}"))
        };
        let direct_caller = entity("direct_caller", EntityKind::Function, ".direct_caller");
        let module_caller = entity("module_caller", EntityKind::Function, ".module_caller");
        let self_caller = entity("self_caller", EntityKind::Function, ".util.self_caller");
        let child = entity("child", EntityKind::Function, ".util.nested.child");
        let method_caller = entity("method_caller", EntityKind::Function, ".method_caller");
        let build = entity("build", EntityKind::Method, ".build");
        let root_helper = entity("helper", EntityKind::Function, ".helper");
        let util_helper = entity("helper", EntityKind::Function, ".util.helper");
        let new_method = entity("new", EntityKind::Method, ".new");
        let run_method = entity("run", EntityKind::Method, ".run");

        let exact_call = |caller_id: &str, callee_id: &str| {
            extraction.edges.iter().any(|edge| {
                edge.relation == RelationKind::Calls
                    && edge.head_id == caller_id
                    && edge.tail_id == callee_id
                    && edge.exactness == Exactness::ParserVerified
                    && edge.source_span.repo_relative_path == "src/lib.rs"
            })
        };

        assert!(exact_call(&direct_caller.id, &root_helper.id));
        assert!(exact_call(&module_caller.id, &util_helper.id));
        assert_eq!(
            extraction
                .edges
                .iter()
                .filter(|edge| {
                    edge.relation == RelationKind::Calls
                        && edge.head_id == module_caller.id
                        && edge.tail_id == util_helper.id
                        && edge.exactness == Exactness::ParserVerified
                })
                .count(),
            3,
            "module path, crate path, and imported alias should all hit util::helper"
        );
        assert!(exact_call(&self_caller.id, &util_helper.id));
        assert!(exact_call(&child.id, &util_helper.id));
        assert!(exact_call(&build.id, &new_method.id));
        assert!(exact_call(&method_caller.id, &new_method.id));
        assert!(exact_call(&method_caller.id, &run_method.id));
    }

    #[test]
    fn rust_trait_macro_and_comment_calls_are_not_exact_overclaims() {
        let source = r#"
pub struct Service;

trait Run {
    fn run(&self);
}

impl Run for Service {
    fn run(&self) {}
}

macro_rules! my_macro {
    () => {};
}

pub fn caller(s: Service) {
    s.run();
    my_macro!();
    let text = "run()";
    // run();
}
"#;
        let extraction = extraction("src/lib.rs", source);
        let caller = extraction
            .entities
            .iter()
            .find(|entity| entity.name == "caller" && entity.kind == EntityKind::Function)
            .expect("caller");
        let run_ids = extraction
            .entities
            .iter()
            .filter(|entity| entity.name == "run")
            .map(|entity| entity.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(
            !run_ids.is_empty(),
            "fixture should contain trait/impl run declarations"
        );
        assert!(
            extraction.edges.iter().all(|edge| {
                !(edge.relation == RelationKind::Calls
                    && edge.head_id == caller.id
                    && run_ids.contains(edge.tail_id.as_str())
                    && edge.exactness == Exactness::ParserVerified)
            }),
            "trait receiver call must not become exact without deterministic trait resolution"
        );
        assert!(
            extraction.edges.iter().any(|edge| {
                edge.relation == RelationKind::Calls
                    && edge.head_id == caller.id
                    && edge.exactness == Exactness::StaticHeuristic
            }),
            "unresolved trait method call should be surfaced as static heuristic"
        );
        assert!(
            extraction
                .edges
                .iter()
                .filter(|edge| edge.relation == RelationKind::Calls && edge.head_id == caller.id)
                .all(|edge| {
                    let tail = extraction
                        .entities
                        .iter()
                        .find(|entity| entity.id == edge.tail_id)
                        .expect("tail entity");
                    !tail.name.contains("my_macro")
                }),
            "macro invocation must not be overclaimed as a CALLS edge"
        );
    }

    #[test]
    fn rust_inline_cfg_tests_emit_test_source_role_metadata() {
        let source = r#"
pub fn prod_value() -> i32 {
    1
}

#[test]
fn standalone_test() {
    prod_value();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same_name() -> i32 {
        prod_value()
    }

    #[test]
    fn calls_prod_value() {
        same_name();
        prod_value();
    }
}
"#;
        let extraction = extraction("src/lib.rs", source);

        let production = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.name == "prod_value" && !entity.qualified_name.contains(".tests.")
            })
            .expect("production function");
        assert_eq!(
            production
                .metadata
                .get("source_role")
                .and_then(serde_json::Value::as_str),
            Some("production")
        );

        let inline_test_entities = extraction
            .entities
            .iter()
            .filter(|entity| {
                entity.qualified_name.contains(".tests.") || entity.name == "standalone_test"
            })
            .collect::<Vec<_>>();
        assert!(
            !inline_test_entities.is_empty(),
            "expected inline test entities"
        );
        assert!(inline_test_entities.iter().all(|entity| {
            entity
                .metadata
                .get("source_role")
                .and_then(serde_json::Value::as_str)
                == Some("test")
        }));
        assert!(inline_test_entities.iter().any(|entity| {
            entity
                .metadata
                .get("source_role_source")
                .and_then(serde_json::Value::as_str)
                == Some("rust_attribute")
                || entity
                    .metadata
                    .get("source_role_source")
                    .and_then(serde_json::Value::as_str)
                    == Some("module_path")
        }));

        let inline_test_calls = extraction
            .edges
            .iter()
            .filter(|edge| {
                edge.relation == RelationKind::Calls
                    && edge.source_span.repo_relative_path == "src/lib.rs"
                    && edge
                        .metadata
                        .get("evidence_role")
                        .and_then(serde_json::Value::as_str)
                        == Some("test")
            })
            .collect::<Vec<_>>();
        assert!(
            !inline_test_calls.is_empty(),
            "inline test calls should not be production evidence"
        );
        assert!(inline_test_calls
            .iter()
            .all(|edge| edge.context == codegraph_core::EdgeContext::Test));
    }

    #[test]
    fn source_role_path_matrix_marks_non_production_evidence() {
        let cases = [
            (
                "src/service.ts",
                "export function serviceValue() { return 1; }\n",
                "production",
            ),
            (
                "tests/service.test.ts",
                "test('service', () => { expect(1).toBe(1); });\n",
                "test",
            ),
            (
                "__tests__/service.spec.js",
                "describe('service', () => { it('works', () => expect(1).toBe(1)); });\n",
                "test",
            ),
            (
                "src/__mocks__/service.ts",
                "export function serviceMock() { return 1; }\n",
                "mock",
            ),
            (
                "src/service.stub.ts",
                "export function serviceStub() { return 1; }\n",
                "mock",
            ),
            (
                "src/generated/client.ts",
                "export function generatedClient() { return 1; }\n",
                "unknown",
            ),
            (
                "vendor/service.ts",
                "export function vendoredService() { return 1; }\n",
                "unknown",
            ),
            ("test_api.py", "def test_api():\n    assert True\n", "test"),
            (
                "pkg/service_test.go",
                "package pkg\nfunc TestService() {}\n",
                "test",
            ),
            (
                "src/test/java/AppTest.java",
                "public class AppTest { void testService() {} }\n",
                "test",
            ),
            (
                "tests/app_test.c",
                "int test_service(void) { return 1; }\n",
                "test",
            ),
        ];

        for (path, source, expected_role) in cases {
            let extraction = extraction(path, source);
            let roles = extraction
                .entities
                .iter()
                .filter_map(|entity| {
                    entity
                        .metadata
                        .get("source_role")
                        .and_then(serde_json::Value::as_str)
                })
                .collect::<BTreeSet<_>>();
            assert!(!roles.is_empty(), "{path} should emit source-role metadata");
            assert!(
                roles.iter().all(|role| *role == expected_role),
                "{path}: expected {expected_role}, got {roles:?}"
            );
            assert!(
                extraction
                    .edges
                    .iter()
                    .filter_map(|edge| {
                        edge.metadata
                            .get("source_role")
                            .and_then(serde_json::Value::as_str)
                    })
                    .all(|role| role == expected_role),
                "{path}: edge roles should match {expected_role}"
            );
        }
    }

    #[test]
    fn tier1_languages_parse_and_report_structural_entities() {
        let cases = [
            (
                "fixtures/App.java",
                "package demo;\nimport java.util.List;\npublic class App { public String run(String input) { return input; } }\n",
                SourceLanguage::Java,
            ),
            (
                "fixtures/App.cs",
                "using System;\nnamespace Demo { public class App { public string Run(string input) { return input; } } }\n",
                SourceLanguage::CSharp,
            ),
            (
                "fixtures/app.c",
                "#include <stdio.h>\nint run(int input) { return input; }\n",
                SourceLanguage::C,
            ),
            (
                "fixtures/app.cpp",
                "#include <string>\nclass App { public: std::string run(std::string input) { return input; } };\n",
                SourceLanguage::Cpp,
            ),
            (
                "fixtures/app.rb",
                "require 'json'\nclass App\n  def run(input)\n    input\n  end\nend\n",
                SourceLanguage::Ruby,
            ),
            (
                "fixtures/app.php",
                "<?php\nnamespace Demo;\nuse DateTime;\nclass App { public function run($input) { return $input; } }\n",
                SourceLanguage::Php,
            ),
        ];

        for (path, source, language) in cases {
            let parsed = parsed(path, source);
            assert_eq!(parsed.language, language);
            let extraction = extract_basic_entities(&parsed, source);
            let kinds = extraction
                .entities
                .iter()
                .map(|entity| entity.kind)
                .collect::<BTreeSet<_>>();
            assert!(kinds.contains(&EntityKind::File), "{path}");
            assert!(kinds.contains(&EntityKind::Module), "{path}");
            assert!(
                kinds.contains(&EntityKind::Class) || kinds.contains(&EntityKind::Function),
                "{path} should expose at least one declaration"
            );
            assert!(extraction
                .edges
                .iter()
                .any(|edge| edge.relation == RelationKind::Contains));
        }
    }

    #[test]
    fn active_language_entities_have_source_spans() {
        let cases = [
            (
                "fixtures/sample.py",
                "def helper(value):\n    return value\n",
            ),
            (
                "fixtures/sample.go",
                "package main\nfunc helper(value string) string { return value }\n",
            ),
            (
                "fixtures/sample.rs",
                "pub fn helper(value: String) -> String { value }\n",
            ),
            (
                "fixtures/App.java",
                "class App { String helper(String value) { return value; } }\n",
            ),
            (
                "fixtures/App.cs",
                "class App { string Helper(string value) { return value; } }\n",
            ),
            (
                "fixtures/app.c",
                "int helper(int value) { return value; }\n",
            ),
            (
                "fixtures/app.cpp",
                "int helper(int value) { return value; }\n",
            ),
            ("fixtures/app.rb", "def helper(value)\n  value\nend\n"),
            (
                "fixtures/app.php",
                "<?php function helper($value) { return $value; }\n",
            ),
        ];

        for (path, source) in cases {
            let extraction = extraction(path, source);
            let entity = extraction
                .entities
                .iter()
                .find(|entity| {
                    matches!(
                        entity.kind,
                        EntityKind::Function | EntityKind::Method | EntityKind::Class
                    )
                })
                .unwrap_or_else(|| panic!("{path} should produce a declaration entity"));
            let span = entity.source_span.as_ref().expect("source span");
            assert_eq!(span.repo_relative_path, path);
            assert!(span.start_line >= 1);
            assert!(span.end_line >= span.start_line);
            assert_eq!(entity.created_from, "tree-sitter-language-frontend");
        }
    }

    #[test]
    fn typescript_semantic_resolver_is_optional() {
        use super::{SemanticResolver, TypeScriptSemanticResolver};

        let resolver = TypeScriptSemanticResolver::new("missing-node", "missing-helper.mjs");
        let capabilities = resolver.workspace_capabilities(std::path::Path::new("."));
        assert!(!capabilities.compiler_resolver_available);
        assert_eq!(
            capabilities.exactness_when_available,
            Exactness::CompilerVerified
        );
        let result = resolver.resolve_symbol(std::path::Path::new("."), "src/app.ts", "App");
        assert!(result.is_err());
    }

    #[test]
    #[ignore = "requires Node plus a resolvable typescript package"]
    fn typescript_semantic_resolver_can_prove_alias_and_barrel_when_available() {
        use super::{SemanticResolver, TypeScriptSemanticResolver};

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codegraph-ts-resolver-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).expect("create resolver fixture");
        fs::write(
            root.join("tsconfig.json"),
            r#"{"compilerOptions":{"module":"commonjs","target":"es2020","strict":true},"include":["src/**/*.ts"]}"#,
        )
        .expect("write tsconfig");
        fs::write(
            root.join("src").join("service.ts"),
            "export function canonicalName(value: string): string { return value; }\n",
        )
        .expect("write service");
        fs::write(
            root.join("src").join("barrel.ts"),
            "export { canonicalName as renamedName } from './service';\n",
        )
        .expect("write barrel");
        fs::write(
            root.join("src").join("consumer.ts"),
            "import { renamedName } from './barrel';\nexport const result = renamedName('ok');\n",
        )
        .expect("write consumer");

        let resolver = TypeScriptSemanticResolver::default();
        let alias = resolver
            .resolve_symbol(&root, "src/consumer.ts", "renamedName")
            .expect("resolver available");
        assert!(alias
            .iter()
            .any(|resolution| resolution.exactness == Exactness::CompilerVerified));

        let import = resolver
            .resolve_import(&root, "src/consumer.ts", "barrel")
            .expect("resolver available");
        assert!(import
            .iter()
            .any(|resolution| resolution.exactness == Exactness::CompilerVerified));

        fs::remove_dir_all(root).expect("cleanup resolver fixture");
    }

    #[test]
    fn source_span_line_and_column_are_one_based() {
        let parsed = parsed("fixtures/basic.ts", TS_FIXTURE);
        let root = parsed.root_node;

        assert_eq!(root.source_span.repo_relative_path, "fixtures/basic.ts");
        assert_eq!(root.start_position.line, 1);
        assert_eq!(root.start_position.column, 1);
        assert_eq!(root.source_span.start_line, 1);
        assert_eq!(root.source_span.start_column, Some(1));
        assert!(root.end_position.line >= 6);
    }

    #[test]
    fn syntax_errors_are_reported_without_panic() {
        let source = "export function broken( {\n  return 1;\n";
        let parsed = parsed("fixtures/broken.ts", source);

        assert!(parsed.has_syntax_errors());
        assert!(!parsed.diagnostics.is_empty());
        assert!(parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.node.is_error || diagnostic.node.is_missing));
    }

    #[test]
    fn tier0_valid_files_parse_all_configured_languages_and_spans_are_bounded() {
        let cases = [
            (
                "fixtures/tier0/app.js",
                "import dep from './dep';\nfunction helper(value) { return value; }\nexport function run(value) { return helper(value); }\n",
                SourceLanguage::JavaScript,
            ),
            (
                "fixtures/tier0/app.jsx",
                "import React from 'react';\nexport function View(props) { return <section>{props.title}</section>; }\n",
                SourceLanguage::Jsx,
            ),
            (
                "fixtures/tier0/app.ts",
                "export function run(value: string): string { return value.trim(); }\n",
                SourceLanguage::TypeScript,
            ),
            (
                "fixtures/tier0/app.tsx",
                "type Props = { title: string };\nexport function View(props: Props) { return <section>{props.title}</section>; }\n",
                SourceLanguage::Tsx,
            ),
            (
                "fixtures/tier0/app.rs",
                "use std::fmt;\npub fn run(value: String) -> String { value }\n",
                SourceLanguage::Rust,
            ),
            (
                "fixtures/tier0/app.py",
                "import os\n\ndef run(value):\n    return value\n",
                SourceLanguage::Python,
            ),
            (
                "fixtures/tier0/app.go",
                "package main\nimport \"fmt\"\nfunc run(value string) string { return fmt.Sprint(value) }\n",
                SourceLanguage::Go,
            ),
            (
                "fixtures/tier0/App.java",
                "package demo; import java.util.List; public class App { public String run(String value) { return value; } }\n",
                SourceLanguage::Java,
            ),
            (
                "fixtures/tier0/App.cs",
                "using System; class App { string Run(string value) { return value; } }\n",
                SourceLanguage::CSharp,
            ),
            (
                "fixtures/tier0/app.c",
                "#include <stdio.h>\nint run(int value) { return value; }\n",
                SourceLanguage::C,
            ),
            (
                "fixtures/tier0/app.cpp",
                "#include <string>\nclass App { public: std::string run(std::string value) { return value; } };\n",
                SourceLanguage::Cpp,
            ),
            (
                "fixtures/tier0/app.rb",
                "require \"json\"\ndef run(value)\n  value\nend\n",
                SourceLanguage::Ruby,
            ),
            (
                "fixtures/tier0/app.php",
                "<?php\nnamespace Demo;\nuse DateTime;\nfunction run($value) { return $value; }\n",
                SourceLanguage::Php,
            ),
        ];

        for (path, source, language) in cases {
            assert_eq!(detect_language(path), Some(language));
            let parsed = parsed(path, source);
            assert_eq!(parsed.language, language);
            assert!(
                !parsed.has_syntax_errors(),
                "{path}: {:?}",
                parsed.diagnostics
            );
            assert_eq!(
                parsed.root_node.source_span.repo_relative_path,
                normalize_repo_relative_path(path)
            );
            assert_span_inside_source(path, source, &parsed.root_node.source_span);
            let extraction = extract_basic_entities(&parsed, source);
            assert_eq!(
                extraction
                    .file
                    .metadata
                    .get("parser_status")
                    .and_then(serde_json::Value::as_str),
                Some("parsed"),
                "{path}"
            );
            assert_extraction_spans_inside_source(path, source, &extraction);
            assert!(!extraction.entities.is_empty(), "{path}");
        }
    }

    #[test]
    fn tier0_partial_broken_primary_files_recover_safe_declarations_and_label_parser_status() {
        let cases = [
            (
                "fixtures/tier0/broken.js",
                "import dep from './dep';\nfunction safe() { return 1; }\nfunction broken( {\n",
                "safe",
            ),
            (
                "fixtures/tier0/broken.jsx",
                "function Safe() { return <div />; }\nconst broken = <section>\n",
                "Safe",
            ),
            (
                "fixtures/tier0/broken.ts",
                "export function safe(value: string) { return value; }\nexport function broken( {\n",
                "safe",
            ),
            (
                "fixtures/tier0/broken.tsx",
                "type Props = { label: string };\nfunction Safe(props: Props) { return <div>{props.label}</div>; }\nconst broken = <section>\n",
                "Safe",
            ),
            (
                "fixtures/tier0/broken.rs",
                "pub fn safe() -> i32 { 1 }\npub fn broken( { \n",
                "safe",
            ),
            (
                "fixtures/tier0/broken.py",
                "import os\n\ndef safe():\n    return 1\n\ndef broken(:\n    pass\n",
                "safe",
            ),
            (
                "fixtures/tier0/broken.go",
                "package main\nfunc safe() int { return 1 }\nfunc broken( { \n",
                "safe",
            ),
            (
                "fixtures/tier0/broken.c",
                "int safe(void) { return 1; }\nint broken( { \n",
                "safe",
            ),
            (
                "fixtures/tier0/broken.cpp",
                "int safe(void) { return 1; }\nint broken( { \n",
                "safe",
            ),
        ];

        for (path, source, safe_name) in cases {
            let parsed = parsed(path, source);
            assert!(parsed.has_syntax_errors(), "{path}");
            let extraction = extract_basic_entities(&parsed, source);
            assert_eq!(
                extraction
                    .file
                    .metadata
                    .get("parser_status")
                    .and_then(serde_json::Value::as_str),
                Some("syntax_errors_recovered"),
                "{path}"
            );
            assert_eq!(
                extraction
                    .file
                    .metadata
                    .get("unsupported_behavior_label")
                    .and_then(serde_json::Value::as_str),
                Some("malformed_regions_not_trusted_for_exact_relations"),
                "{path}"
            );
            assert!(
                extraction.entities.iter().any(|entity| {
                    entity.name == safe_name
                        && matches!(entity.kind, EntityKind::Function | EntityKind::Method)
                }),
                "{path} missing recovered declaration {safe_name}"
            );
            assert_extraction_spans_inside_source(path, source, &extraction);
        }
    }

    #[test]
    fn tier0_fixture_shaped_broken_exports_recover_only_untrusted_declarations() {
        let cases = [
            (
                "fixtures/language_coverage_matrix/partial_broken_code/typescript/src/broken.ts",
                "export function broken(value: string) {\n  if (value) {\n    return value.trim()\n",
                SourceLanguage::TypeScript,
            ),
            (
                "fixtures/language_coverage_matrix/partial_broken_code/ruby/src/broken.rb",
                "def broken(value)\n  if value\n    value.strip\n",
                SourceLanguage::Ruby,
            ),
        ];

        for (path, source, language) in cases {
            let parsed = parsed(path, source);
            assert_eq!(parsed.language, language);
            assert!(parsed.has_syntax_errors(), "{path}");
            let extraction = extract_basic_entities(&parsed, source);
            let broken = extraction
                .entities
                .iter()
                .find(|entity| entity.kind == EntityKind::Function && entity.name == "broken")
                .expect("broken declaration should be recovered as untrusted syntax evidence");
            assert_eq!(
                broken
                    .metadata
                    .get("syntax_recovery")
                    .and_then(serde_json::Value::as_bool),
                Some(true),
                "{path}"
            );
            assert_eq!(
                broken
                    .metadata
                    .get("parser_reliability")
                    .and_then(serde_json::Value::as_str),
                Some("untrusted_syntax_region"),
                "{path}"
            );
            assert_extraction_spans_inside_source(path, source, &extraction);
            assert!(!extraction.edges.iter().any(|edge| {
                edge.tail_id == broken.id
                    && matches!(
                        edge.relation,
                        RelationKind::Defines | RelationKind::Declares
                    )
                    && edge.exactness == Exactness::ParserVerified
            }));
        }
    }

    #[test]
    fn tier0_malformed_constructs_do_not_emit_exact_calls_from_error_nodes() {
        let source = "function target() { return 1; }\nfunction broken() {\n  return target(\n}\n";
        let parsed = parsed("fixtures/tier0/malformed_call.ts", source);
        assert!(parsed.has_syntax_errors());
        let extraction = extract_basic_entities(&parsed, source);
        let target = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "target")
            .expect("target function");

        assert!(!extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Calls
                && edge.tail_id == target.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.start_line >= 3
        }));

        let broken = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "broken")
            .expect("broken function entity is recoverable");
        assert_eq!(
            broken
                .metadata
                .get("parser_reliability")
                .and_then(serde_json::Value::as_str),
            Some("untrusted_syntax_region")
        );
    }

    #[test]
    fn tier0_macro_and_preprocessor_expansions_are_not_exact_generated_facts() {
        let rust_source = "macro_rules! make_fn { ($name:ident) => { fn $name() {} }; }\nmake_fn!(generated);\nfn real() {}\n";
        let rust = extraction("fixtures/tier0/macro.rs", rust_source);
        assert!(rust
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Function && entity.name == "real"));
        assert!(!rust
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Function && entity.name == "generated"));

        let c_source =
            "#define MAKE_FN(name) int name(void) { return 1; }\nMAKE_FN(generated)\nint real(void) { return 2; }\n";
        let c = extraction("fixtures/tier0/macro.c", c_source);
        assert!(c
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Function && entity.name == "real"));
        assert!(!c
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Function && entity.name == "generated"));
        assert!(!c
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Function && entity.name == "MAKE_FN"));
    }

    #[test]
    fn tier0_parser_crash_prevention_for_malformed_inputs() {
        let cases = [
            ("fixtures/tier0/crash.js", "function broken( {\n"),
            ("fixtures/tier0/crash.ts", "export const value: = ;\n"),
            ("fixtures/tier0/crash.tsx", "const view = <section>\n"),
            ("fixtures/tier0/crash.rs", "pub fn broken( { \n"),
            ("fixtures/tier0/crash.py", "def broken(:\n"),
            ("fixtures/tier0/crash.go", "package main\nfunc broken( { \n"),
            ("fixtures/tier0/crash.c", "int broken( { \n"),
            ("fixtures/tier0/crash.cpp", "template <\n"),
        ];

        for (path, source) in cases {
            let result = std::panic::catch_unwind(|| {
                let parsed = parser().parse(path, source);
                if let Ok(Some(parsed)) = parsed {
                    let _ = extract_basic_entities(&parsed, source);
                }
            });
            assert!(result.is_ok(), "{path} panicked");
        }
    }

    #[test]
    fn tier1_js_ts_import_export_forms_have_binding_entities_and_claim_labels() {
        let source = "import defaultThing, { target as aliasTarget, other } from './lib';\n\
import * as ns from './namespace';\n\
import './side-effect';\n\
export { target as exportedTarget } from './lib';\n\
export * from './wildcard';\n\
export default function run() { return aliasTarget(); }\n";
        let extraction = extraction("fixtures/tier1/use.ts", source);

        let default_import = import_entity(&extraction, "defaultThing", "default");
        assert_eq!(
            metadata_str(default_import, "imported_name"),
            Some("default")
        );
        assert_eq!(
            metadata_str(default_import, "module_specifier"),
            Some("./lib")
        );
        assert_eq!(
            metadata_str(default_import, "target_resolution_claim_state"),
            Some("unresolved")
        );
        assert_eq!(
            metadata_str(default_import, "syntax_claim_state"),
            Some("exact")
        );

        let named_alias = import_entity(&extraction, "aliasTarget", "named");
        assert_eq!(metadata_str(named_alias, "imported_name"), Some("target"));
        assert_eq!(
            metadata_str(named_alias, "resolution"),
            Some("parser_observed_unresolved_static_import")
        );

        let namespace = import_entity(&extraction, "ns", "namespace");
        assert_eq!(metadata_str(namespace, "imported_name"), Some("*"));

        let side_effect = import_entity(&extraction, "./side-effect", "side_effect");
        assert_eq!(
            metadata_str(side_effect, "module_specifier"),
            Some("./side-effect")
        );

        let reexport = import_entity(&extraction, "exportedTarget", "named_reexport");
        assert_eq!(metadata_str(reexport, "imported_name"), Some("target"));
        let wildcard = import_entity(&extraction, "*", "wildcard_reexport");
        assert_eq!(
            metadata_str(wildcard, "target_resolution_claim_state"),
            Some("unsupported")
        );
        assert!(metadata_str(wildcard, "unsupported_reason")
            .is_some_and(|reason| reason.contains("wildcard re-export")));

        let export_entities = extraction
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Export)
            .collect::<Vec<_>>();
        assert!(export_entities
            .iter()
            .any(|entity| metadata_str(entity, "export_kind") == Some("named_reexport")));
        assert!(export_entities
            .iter()
            .any(
                |entity| metadata_str(entity, "export_kind") == Some("wildcard_reexport")
                    && metadata_str(entity, "target_resolution_claim_state") == Some("unsupported")
            ));
        assert!(extraction
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Import)
            .all(|entity| metadata_str(entity, "resolution") != Some("resolved_static_import")));
        assert_extraction_spans_inside_source("fixtures/tier1/use.ts", source, &extraction);
    }

    #[test]
    fn tier1_jsx_and_tsx_import_export_forms_match_js_ts_claim_boundaries() {
        let jsx_source = "import React, { useMemo as memo } from 'react';\n\
import * as widgets from './widgets';\n\
export function View() { return <widgets.Panel>{memo(() => 1, [])}</widgets.Panel>; }\n\
export { View as ExportedView };\n";
        let jsx = extraction("fixtures/tier1/view.jsx", jsx_source);
        let react_default = import_entity(&jsx, "React", "default");
        assert_eq!(
            metadata_str(react_default, "module_specifier"),
            Some("react")
        );
        let memo = import_entity(&jsx, "memo", "named");
        assert_eq!(metadata_str(memo, "imported_name"), Some("useMemo"));
        let widgets = import_entity(&jsx, "widgets", "namespace");
        assert_eq!(metadata_str(widgets, "imported_name"), Some("*"));
        assert!(jsx
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Export
                && entity.name == "ExportedView"
                && metadata_str(entity, "export_kind") == Some("named_export")));
        assert!(jsx
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Import)
            .all(|entity| metadata_str(entity, "target_resolution_claim_state") != Some("exact")));
        assert_extraction_spans_inside_source("fixtures/tier1/view.jsx", jsx_source, &jsx);

        let tsx_source = "import type { Props as ViewProps } from './types';\n\
import defaultWidget, * as widgets from './widgets';\n\
export default function View(props: ViewProps) { return <widgets.Panel />; }\n";
        let tsx = extraction("fixtures/tier1/view.tsx", tsx_source);
        let props = import_entity(&tsx, "ViewProps", "named");
        assert_eq!(metadata_str(props, "imported_name"), Some("Props"));
        assert_eq!(metadata_str(props, "module_specifier"), Some("./types"));
        let default_widget = import_entity(&tsx, "defaultWidget", "default");
        assert_eq!(
            metadata_str(default_widget, "module_specifier"),
            Some("./widgets")
        );
        let widgets = import_entity(&tsx, "widgets", "namespace");
        assert_eq!(metadata_str(widgets, "imported_name"), Some("*"));
        assert!(tsx
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Import)
            .all(|entity| metadata_str(entity, "resolution")
                == Some("parser_observed_unresolved_static_import")));
        assert_extraction_spans_inside_source("fixtures/tier1/view.tsx", tsx_source, &tsx);
    }

    #[test]
    fn tier1_primary_language_import_aliases_are_declared_with_honest_target_labels() {
        let cases = [
            (
                "fixtures/tier1/use.py",
                "import os as operating_system\nfrom pkg.service import target as aliased_target\n",
                SourceLanguage::Python,
                "aliased_target",
                "python_from_import",
                "target",
                "pkg.service",
            ),
            (
                "fixtures/tier1/lib.rs",
                "use crate::target as aliased_target;\n",
                SourceLanguage::Rust,
                "aliased_target",
                "rust_use",
                "crate::target",
                "crate::target",
            ),
            (
                "fixtures/tier1/main.go",
                "package main\nimport fmtalias \"fmt\"\n",
                SourceLanguage::Go,
                "fmtalias",
                "go_import",
                "fmt",
                "fmt",
            ),
        ];

        for (path, source, language, local, import_kind, imported, module) in cases {
            let parsed = parsed(path, source);
            assert_eq!(parsed.language, language);
            let extraction = extract_basic_entities(&parsed, source);
            let entity = import_entity(&extraction, local, import_kind);
            assert_eq!(metadata_str(entity, "imported_name"), Some(imported));
            assert_eq!(metadata_str(entity, "module_specifier"), Some(module));
            assert_eq!(
                metadata_str(entity, "target_resolution_claim_state"),
                Some("unsupported"),
                "{path}"
            );
            assert_eq!(metadata_str(entity, "syntax_claim_state"), Some("exact"));
            if language == SourceLanguage::Python {
                assert!(
                    extraction.edges.iter().any(|edge| {
                        edge.relation == RelationKind::Imports
                            && edge.exactness == Exactness::StaticHeuristic
                            && extraction.entities.iter().any(|entity| {
                                entity.id == edge.tail_id
                                    && entity.kind == EntityKind::Import
                                    && entity.name == "pkg.service.target"
                            })
                    }),
                    "Python from-import target must emit a non-proof unresolved import edge"
                );
            }
            assert_extraction_spans_inside_source(path, source, &extraction);
        }
    }

    #[test]
    fn tier1_c_and_cpp_includes_are_text_evidence_without_exact_target_resolution() {
        let cases = [
            (
                "fixtures/tier1/include.c",
                "#include <stdio.h>\nint main(void) { return 0; }\n",
                "stdio.h",
                "c_include",
            ),
            (
                "fixtures/tier1/include.cpp",
                "#include \"local.hpp\"\nint main() { return 0; }\n",
                "local.hpp",
                "cpp_include",
            ),
        ];

        for (path, source, local, import_kind) in cases {
            let extraction = extraction(path, source);
            let include = import_entity(&extraction, local, import_kind);
            assert_eq!(metadata_str(include, "claim_state"), Some("unsupported"));
            assert_eq!(
                metadata_str(include, "target_resolution_claim_state"),
                Some("unsupported")
            );
            assert_eq!(
                metadata_str(include, "resolution"),
                Some("unresolved_preprocessor_include")
            );
            assert_extraction_spans_inside_source(path, source, &extraction);
        }
    }

    #[test]
    fn tier1_secondary_import_forms_are_declared_when_parser_exposes_import_nodes() {
        let cases = [
            (
                "fixtures/tier1/App.java",
                "package demo; import java.util.List; public class App {}\n",
                "List",
                "java_import",
            ),
            (
                "fixtures/tier1/App.cs",
                "using Alias = System.Text.StringBuilder; public class App {}\n",
                "Alias",
                "csharp_using",
            ),
            (
                "fixtures/tier1/app.php",
                "<?php\nnamespace Demo;\nuse DateTime as Clock;\nfunction run() {}\n",
                "Clock",
                "php_use",
            ),
        ];

        for (path, source, local, import_kind) in cases {
            let extraction = extraction(path, source);
            let entity = import_entity(&extraction, local, import_kind);
            assert_eq!(
                metadata_str(entity, "target_resolution_claim_state"),
                Some("unsupported"),
                "{path}"
            );
            assert_extraction_spans_inside_source(path, source, &extraction);
        }
    }

    #[test]
    fn tier1_declarations_preserve_file_identity_for_same_name_symbols() {
        let first = extraction(
            "fixtures/tier1/one.ts",
            "export function duplicate() { return 1; }\n",
        );
        let second = extraction(
            "fixtures/tier1/two.ts",
            "export function duplicate() { return 2; }\n",
        );
        let first_duplicate = first
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "duplicate")
            .expect("first duplicate");
        let second_duplicate = second
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "duplicate")
            .expect("second duplicate");

        assert_ne!(first_duplicate.id, second_duplicate.id);
        assert_ne!(
            first_duplicate.repo_relative_path,
            second_duplicate.repo_relative_path
        );
        assert_extraction_spans_inside_source(
            "fixtures/tier1/one.ts",
            "export function duplicate() { return 1; }\n",
            &first,
        );
        assert_extraction_spans_inside_source(
            "fixtures/tier1/two.ts",
            "export function duplicate() { return 2; }\n",
            &second,
        );
    }

    #[test]
    fn tier1_cpp_class_specifier_declares_class_entity() {
        let source = "namespace fixture { class Service { public: int value() { return 1; } }; }\n";
        let extraction = extraction("fixtures/tier1/nested.cpp", source);

        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Class && entity.name == "Service"));
        assert_extraction_spans_inside_source("fixtures/tier1/nested.cpp", source, &extraction);
    }

    #[test]
    fn extracts_simple_function_entities() {
        let extraction = extraction("fixtures/simple_function.ts", SIMPLE_FUNCTION);
        let kinds = extraction
            .entities
            .iter()
            .map(|entity| entity.kind)
            .collect::<BTreeSet<_>>();

        assert!(kinds.contains(&EntityKind::File));
        assert!(kinds.contains(&EntityKind::Module));
        assert!(kinds.contains(&EntityKind::Function));
        assert!(kinds.contains(&EntityKind::Parameter));
        assert!(kinds.contains(&EntityKind::LocalVariable));
        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.qualified_name.ends_with(".add")));
    }

    #[test]
    fn extracts_class_methods_constructor_and_parameters() {
        let extraction = extraction("fixtures/class_with_methods.ts", CLASS_FIXTURE);

        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Class && entity.name == "Counter"));
        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Constructor && entity.name == "constructor"));
        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Method && entity.name == "increment"));
        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Parameter && entity.name == "step"));
    }

    #[test]
    fn extracts_import_export_entities_and_edges() {
        let extraction = extraction("fixtures/imports_exports.ts", IMPORT_EXPORT_FIXTURE);

        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Import));
        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Export));
        assert!(extraction
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Imports));
        assert!(extraction
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Exports));
    }

    #[test]
    fn nested_function_source_spans_are_line_accurate() {
        let extraction = extraction("fixtures/nested_functions.ts", NESTED_FUNCTIONS);
        let inner = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "inner")
            .expect("inner function entity");
        let span = inner.source_span.as_ref().expect("inner source span");

        assert_eq!(span.start_line, 2);
        assert!(span.end_line >= 4);
    }

    #[test]
    fn extracted_relations_include_structural_edges() {
        let extraction = extraction("fixtures/class_with_methods.ts", CLASS_FIXTURE);
        let relations = extraction
            .edges
            .iter()
            .map(|edge| edge.relation)
            .collect::<BTreeSet<_>>();

        assert!(relations.contains(&RelationKind::Contains));
        assert!(relations.contains(&RelationKind::DefinedIn));
        assert!(relations.contains(&RelationKind::Defines));
        assert!(relations.contains(&RelationKind::Declares));
    }

    #[test]
    fn function_calls_produce_callsite_callee_and_argument_edges() {
        let extraction = extraction("fixtures/core_relations.ts", CORE_RELATIONS);
        let relations = extraction
            .edges
            .iter()
            .map(|edge| edge.relation)
            .collect::<BTreeSet<_>>();

        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::CallSite));
        assert!(relations.contains(&RelationKind::Calls));
        assert!(relations.contains(&RelationKind::Callee));
        assert!(relations.contains(&RelationKind::Argument0));
        assert!(extraction
            .edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Calls)
            .all(|edge| edge.exactness == codegraph_core::Exactness::ParserVerified));
    }

    #[test]
    fn call_edges_use_exact_call_expression_span() {
        let source = "\
function first() { return 'first'; }
function second() { return 'second'; }
export function run(flag: boolean) {
  first();
  if (flag) {
    second();
  }
}
";
        let extraction = extraction("fixtures/exact_callsite.ts", source);
        let second = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "second")
            .expect("second function");
        let edge = extraction
            .edges
            .iter()
            .find(|edge| edge.relation == RelationKind::Calls && edge.tail_id == second.id)
            .expect("CALLS edge to second");

        assert_eq!(
            edge.source_span.repo_relative_path,
            "fixtures/exact_callsite.ts"
        );
        assert_eq!(edge.source_span.start_line, 6);
        assert_eq!(edge.source_span.start_column, Some(5));
        assert_eq!(edge.source_span.end_line, 6);
        assert_eq!(edge.source_span.end_column, Some(13));
    }

    #[test]
    fn assertion_edges_use_exact_assertion_expression_span() {
        let assertion = "assert.equal(second(), \"second\")";
        let source = "\
it(\"checks values\", () => {
  assert.equal(first(), \"first\");
  assert.equal(second(), \"second\");
});
";
        let extraction = extraction("fixtures/exact_assertion.spec.ts", source);
        let edge = extraction
            .edges
            .iter()
            .find(|edge| edge.relation == RelationKind::Asserts && edge.source_span.start_line == 3)
            .expect("ASSERTS edge for second assertion");

        assert_eq!(
            edge.source_span.repo_relative_path,
            "fixtures/exact_assertion.spec.ts"
        );
        assert_eq!(edge.source_span.start_column, Some(3));
        assert_eq!(edge.source_span.end_line, 3);
        assert_eq!(
            edge.source_span.end_column,
            Some(3 + assertion.chars().count() as u32)
        );
    }

    #[test]
    fn tier4_js_ts_test_blocks_emit_test_assert_and_mock_roles() {
        let source = "\
import { describe, it, expect, vi } from 'vitest';
function subject() { return 1; }
describe('subject', () => {
  it('works', () => {
    vi.mock('./net');
    expect(subject()).toBe(1);
  });
});
";
        let extraction = extraction("tests/service.spec.ts", source);
        let subject = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "subject")
            .expect("subject function");
        let test_case = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::TestCase && entity.name == "works")
            .expect("test case");

        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::TestFile));
        assert_eq!(
            subject
                .metadata
                .get("source_role")
                .and_then(serde_json::Value::as_str),
            Some("test")
        );
        assert!(extraction.edges.iter().any(|edge| {
            edge.head_id == test_case.id
                && edge.tail_id == subject.id
                && edge.relation == RelationKind::Tests
                && edge
                    .metadata
                    .get("source_role")
                    .and_then(serde_json::Value::as_str)
                    == Some("test")
        }));
        assert!(extraction
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Asserts));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Mocks
                && edge
                    .metadata
                    .get("source_role")
                    .and_then(serde_json::Value::as_str)
                    == Some("mock")
        }));
        let test_case_span = test_case.source_span.as_ref().expect("test case span");
        assert_eq!(test_case_span.start_line, 4);
        assert_eq!(test_case_span.start_column, Some(3));
        let assert_edge = extraction
            .edges
            .iter()
            .find(|edge| edge.relation == RelationKind::Asserts)
            .expect("assertion edge");
        assert_eq!(assert_edge.source_span.start_line, 6);
        assert_eq!(assert_edge.source_span.start_column, Some(5));
        let mock_edge = extraction
            .edges
            .iter()
            .find(|edge| edge.relation == RelationKind::Mocks)
            .expect("mock edge");
        assert_eq!(mock_edge.source_span.start_line, 5);
        assert_eq!(mock_edge.source_span.start_column, Some(5));
    }

    #[test]
    fn tier4_generic_primary_test_files_emit_testcase_assert_mock_and_tests_edges() {
        let python = "\
def subject():
    return 1

def test_subject(monkeypatch):
    monkeypatch.setattr('svc.value', lambda: 1)
    assert subject() == 1
";
        let py = extraction("tests/test_service.py", python);
        assert!(py
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::TestFile));
        let py_subject = py
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "subject")
            .expect("python subject");
        let py_test = py
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::TestCase && entity.name == "test_subject")
            .expect("python test case");
        assert!(py.edges.iter().any(|edge| {
            edge.head_id == py_test.id
                && edge.tail_id == py_subject.id
                && edge.relation == RelationKind::Tests
        }));
        assert!(py
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Asserts));
        assert!(py.edges.iter().any(|edge| {
            edge.relation == RelationKind::Mocks
                && edge.exactness == Exactness::StaticHeuristic
                && edge
                    .metadata
                    .get("source_role")
                    .and_then(serde_json::Value::as_str)
                    == Some("mock")
        }));

        let go = "\
package service
import \"testing\"
func subject() int { return 1 }
func TestSubject(t *testing.T) {
  if subject() != 1 {
    t.Fatal(\"bad\")
  }
}
";
        let go_extraction = extraction("service_test.go", go);
        let go_subject = go_extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "subject")
            .expect("go subject");
        let go_test = go_extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::TestCase && entity.name == "TestSubject")
            .expect("go test case");
        assert!(go_extraction.edges.iter().any(|edge| {
            edge.head_id == go_test.id
                && edge.tail_id == go_subject.id
                && edge.relation == RelationKind::Tests
        }));
        assert!(go_extraction
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Asserts));
    }

    #[test]
    fn tier4_rust_inline_tests_keep_source_role_and_assertion_evidence() {
        let source = "\
pub fn subject() -> i32 { 1 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {
        assert_eq!(subject(), 1);
    }
}
";
        let extraction = extraction("src/lib.rs", source);
        let works = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "works")
            .expect("rust works function");
        assert_eq!(
            works
                .metadata
                .get("source_role")
                .and_then(serde_json::Value::as_str),
            Some("test")
        );
        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::TestCase && entity.name == "works"));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Asserts
                && edge
                    .metadata
                    .get("source_role")
                    .and_then(serde_json::Value::as_str)
                    == Some("test")
        }));
    }

    #[test]
    fn tier4_common_secondary_test_paths_are_classified_low_risk() {
        for path in [
            "src/test/java/AuthTest.java",
            "tests/AuthSpec.cs",
            "spec/service_spec.rb",
            "tests/AuthTest.php",
            "pkg/service_test.go",
            "tests/test_service.py",
        ] {
            assert!(super::is_test_file_path(path), "{path}");
        }
    }

    #[test]
    fn table_constant_return_from_writer_produces_write_to_table() {
        let source = "\
export const ordersTable = \"orders\";

export function saveOrder(order: any) {
  return ordersTable;
}
";
        let extraction = extraction("src/store.ts", source);
        let table = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Table
                    && entity.name == "ordersTable"
                    && entity.qualified_name == "src::store.ordersTable"
            })
            .expect("ordersTable table entity");
        let save_order = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Function
                    && entity.name == "saveOrder"
                    && entity.qualified_name == "src::store.saveOrder"
            })
            .expect("saveOrder function");
        let write = extraction
            .edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::Writes
                    && edge.head_id == save_order.id
                    && edge.tail_id == table.id
            })
            .expect("WRITES saveOrder -> ordersTable");

        assert_eq!(write.exactness, Exactness::ParserVerified);
        assert_eq!(write.source_span.repo_relative_path, "src/store.ts");
        assert_eq!(write.source_span.start_line, 4);
        assert_eq!(write.source_span.start_column, Some(10));
        assert_eq!(write.source_span.end_line, 4);
        assert_eq!(write.source_span.end_column, Some(21));
    }

    #[test]
    fn dynamic_call_targets_remain_heuristic() {
        let source = "\
export function run(registry: Record<string, Function>, name: string) {
  return registry[name]();
}
";
        let extraction = extraction("fixtures/dynamic_call.ts", source);
        let run = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
            .expect("run function");
        let call = extraction
            .edges
            .iter()
            .find(|edge| edge.relation == RelationKind::Calls && edge.head_id == run.id)
            .expect("CALLS edge");

        assert_eq!(call.exactness, Exactness::StaticHeuristic);
        assert!(call.confidence < 1.0);
    }

    #[test]
    fn ambiguous_same_scope_call_target_remains_heuristic() {
        let source = "\
function target() { return 'first'; }
function target() { return 'second'; }
export function run() {
  return target();
}
";
        let extraction = extraction("fixtures/ambiguous_call.ts", source);
        let run = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
            .expect("run function");
        let declaration_ids = extraction
            .entities
            .iter()
            .filter(|entity| {
                entity.kind == EntityKind::Function
                    && entity.name == "target"
                    && entity.created_from != "tree-sitter-static-heuristic"
            })
            .map(|entity| entity.id.clone())
            .collect::<BTreeSet<_>>();
        let call = extraction
            .edges
            .iter()
            .find(|edge| edge.relation == RelationKind::Calls && edge.head_id == run.id)
            .expect("CALLS edge");

        assert_eq!(declaration_ids.len(), 2);
        assert_eq!(call.exactness, Exactness::StaticHeuristic);
        assert!(call.confidence < 1.0);
        assert!(!declaration_ids.contains(&call.tail_id));
    }

    #[test]
    fn multiple_call_arguments_produce_numbered_argument_edges() {
        let source = "\
function target(first: string, second: string, third: string) {
  return first;
}

export function demo(x: string, y: string, z: string) {
  target(x, y, z);
}
";
        let extraction = extraction("fixtures/multiple_arguments.ts", source);
        let relations = extraction
            .edges
            .iter()
            .map(|edge| edge.relation)
            .collect::<BTreeSet<_>>();

        assert!(relations.contains(&RelationKind::Argument0));
        assert!(relations.contains(&RelationKind::Argument1));
        assert!(relations.contains(&RelationKind::ArgumentN));
    }

    #[test]
    fn assignments_produce_writes_assigned_from_and_flows() {
        let extraction = extraction("fixtures/core_relations.ts", CORE_RELATIONS);
        let relations = extraction
            .edges
            .iter()
            .map(|edge| edge.relation)
            .collect::<BTreeSet<_>>();

        assert!(relations.contains(&RelationKind::Writes));
        assert!(relations.contains(&RelationKind::AssignedFrom));
        assert!(relations.contains(&RelationKind::FlowsTo));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::FlowsTo
                && entity_name(&extraction, &edge.head_id) == Some("input")
                && entity_name(&extraction, &edge.tail_id) == Some("a")
        }));
    }

    #[test]
    fn member_assignments_produce_mutates_edges() {
        let source = "\
export function demo(box: { value: string }, input: string) {
  box.value = input;
}
";
        let extraction = extraction("fixtures/member_assignment.ts", source);

        assert!(extraction
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Mutates));
    }

    #[test]
    fn obvious_mutation_method_calls_produce_heuristic_mutates_edges() {
        let source = "\
export function demo(items: string[], input: string) {
  items.push(input);
}
";
        let extraction = extraction("fixtures/mutation_method.ts", source);

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Mutates
                && entity_name(&extraction, &edge.tail_id) == Some("items")
                && edge.exactness == Exactness::StaticHeuristic
                && edge.confidence < 1.0
        }));
    }

    #[test]
    fn variable_references_produce_reads_where_reliable() {
        let extraction = extraction("fixtures/core_relations.ts", CORE_RELATIONS);

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Reads
                && entity_name(&extraction, &edge.tail_id) == Some("a")
        }));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Reads
                && entity_name(&extraction, &edge.tail_id) == Some("b")
        }));
    }

    #[test]
    fn call_argument_flows_into_argument_slot() {
        let extraction = extraction("fixtures/core_relations.ts", CORE_RELATIONS);
        let argument_targets = extraction
            .edges
            .iter()
            .filter(|edge| edge.relation == RelationKind::Argument0)
            .map(|edge| edge.tail_id.as_str())
            .collect::<BTreeSet<_>>();

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::FlowsTo
                && entity_name(&extraction, &edge.head_id) == Some("b")
                && argument_targets.contains(edge.tail_id.as_str())
        }));
    }

    #[test]
    fn direct_call_argument_flows_into_known_callee_parameter() {
        let extraction = extraction("fixtures/core_relations.ts", CORE_RELATIONS);

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::FlowsTo
                && entity_name(&extraction, &edge.head_id) == Some("b")
                && entity_name(&extraction, &edge.tail_id) == Some("value")
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.repo_relative_path == "fixtures/core_relations.ts"
                && edge.source_span.start_line == 9
                && edge.source_span.start_column == Some(10)
        }));
    }

    #[test]
    fn tier3_return_value_flow_from_direct_call_assignment_is_exact() {
        let source = "\
function helper(x: string) {
  return x;
}

export function run(input: string) {
  const value = helper(input);
  return value;
}
";
        let extraction = extraction("fixtures/tier3/return_value_flow.ts", source);
        assert_extraction_spans_inside_source(
            "fixtures/tier3/return_value_flow.ts",
            source,
            &extraction,
        );
        let return_site = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::ReturnSite
                    && entity.repo_relative_path == "fixtures/tier3/return_value_flow.ts"
                    && entity
                        .source_span
                        .as_ref()
                        .is_some_and(|span| span.start_line == 2)
            })
            .expect("helper return site");
        let value = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::LocalVariable && entity.name == "value")
            .expect("value local");
        assert!(
            extraction.edges.iter().any(|edge| {
                edge.relation == RelationKind::FlowsTo
                    && edge.head_id == return_site.id
                    && edge.tail_id == value.id
                    && edge.exactness == Exactness::ParserVerified
                    && edge.source_span.start_line == 6
                    && edge.source_span.start_column == Some(17)
            }),
            "missing exact helper return-site to assigned local flow"
        );
    }

    #[test]
    fn tier3_dynamic_return_value_assignment_does_not_emit_exact_return_flow() {
        let source = "\
function helper(x: string) {
  return x;
}

export function run(registry: Record<string, (x: string) => string>, name: string, input: string) {
  const value = registry[name](input);
  return value;
}
";
        let extraction = extraction("fixtures/tier3/dynamic_return_value_flow.ts", source);
        let return_site = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::ReturnSite
                    && entity.repo_relative_path == "fixtures/tier3/dynamic_return_value_flow.ts"
                    && entity
                        .source_span
                        .as_ref()
                        .is_some_and(|span| span.start_line == 2)
            })
            .expect("helper return site");
        let value = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::LocalVariable && entity.name == "value")
            .expect("value local");

        assert!(!extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::FlowsTo
                && edge.head_id == return_site.id
                && edge.tail_id == value.id
                && edge.exactness == Exactness::ParserVerified
        }));
    }

    #[test]
    fn return_statements_produce_returnsite_and_returns_edges() {
        let extraction = extraction("fixtures/core_relations.ts", CORE_RELATIONS);
        let return_sites = extraction
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::ReturnSite)
            .collect::<Vec<_>>();

        assert!(!return_sites.is_empty());
        assert!(extraction
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Returns));
        assert!(extraction
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::ReturnsTo));
    }

    #[test]
    fn generic_return_statements_produce_returnsite_and_value_flow() {
        let source = "def helper(value):\n    return value\n";
        let extraction = extraction("fixtures/return_flow.py", source);
        let return_site = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::ReturnSite)
            .expect("return site");

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::FlowsTo
                && entity_name(&extraction, &edge.head_id) == Some("value")
                && edge.tail_id == return_site.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.repo_relative_path == "fixtures/return_flow.py"
                && edge.source_span.start_line == 2
        }));
    }

    #[test]
    fn unresolved_calls_are_marked_static_heuristic() {
        let cases = [
            (
                "fixtures/unresolved_call.ts",
                "export function demo(input: string) {\n  return missingCall(input);\n}\n",
                "missingCall",
            ),
            (
                "fixtures/unresolved_call.go",
                "package main\n\nfunc demo(input int) int {\n    return missingLocalGo(input)\n}\n",
                "missingLocalGo",
            ),
        ];

        for (path, source, missing_name) in cases {
            let extraction = extraction(path, source);

            assert!(
                extraction.edges.iter().any(|edge| {
                    edge.relation == RelationKind::Calls
                        && edge.exactness == codegraph_core::Exactness::StaticHeuristic
                        && edge.confidence < 1.0
                        && entity_name(&extraction, &edge.tail_id) == Some(missing_name)
                }),
                "{path}: expected unresolved call to `{missing_name}`; got {:?}",
                extraction.edges
            );
        }
    }

    #[test]
    fn tier2_primary_direct_calls_have_exact_targets_and_valid_spans() {
        let cases = [
            (
                "fixtures/tier2/direct.js",
                "function helper(value) { return value; }\nexport function run(input) {\n  return helper(input);\n}\n",
            ),
            (
                "fixtures/tier2/direct.ts",
                "function helper(value: string): string { return value; }\nexport function run(input: string): string {\n  return helper(input);\n}\n",
            ),
            (
                "fixtures/tier2/direct.jsx",
                "function helper(value) { return value; }\nexport function run(props) {\n  helper(props.value);\n  return <span>{props.value}</span>;\n}\n",
            ),
            (
                "fixtures/tier2/direct.tsx",
                "function helper(value: string): string { return value; }\nexport function run(props: { value: string }) {\n  helper(props.value);\n  return <span>{props.value}</span>;\n}\n",
            ),
            (
                "fixtures/tier2/direct.py",
                "def helper(value):\n    return value\n\ndef run(input):\n    return helper(input)\n",
            ),
            (
                "fixtures/tier2/direct.go",
                "package main\n\nfunc helper(value int) int { return value }\nfunc run(input int) int {\n    return helper(input)\n}\n",
            ),
            (
                "fixtures/tier2/direct.rs",
                "fn helper(value: i32) -> i32 { value }\nfn run(input: i32) -> i32 {\n    helper(input)\n}\n",
            ),
        ];

        for (path, source) in cases {
            let extraction = extraction(path, source);
            assert_extraction_spans_inside_source(path, source, &extraction);
            let helper = extraction
                .entities
                .iter()
                .find(|entity| {
                    entity.kind == EntityKind::Function
                        && entity.name == "helper"
                        && entity.created_from != "tree-sitter-static-heuristic"
                })
                .unwrap_or_else(|| panic!("{path}: missing helper function"));
            let run = extraction
                .entities
                .iter()
                .find(|entity| {
                    entity.kind == EntityKind::Function
                        && entity.name == "run"
                        && entity.created_from != "tree-sitter-static-heuristic"
                })
                .unwrap_or_else(|| panic!("{path}: missing run function"));
            let call = extraction
                .edges
                .iter()
                .find(|edge| {
                    edge.relation == RelationKind::Calls
                        && edge.head_id == run.id
                        && edge.tail_id == helper.id
                })
                .unwrap_or_else(|| panic!("{path}: missing exact helper CALLS edge"));

            assert_eq!(call.exactness, Exactness::ParserVerified, "{path}");
            assert_eq!(call.confidence, 1.0, "{path}");
            assert_span_inside_source(path, source, &call.source_span);
        }
    }

    #[test]
    fn tier2_generic_primary_reads_and_writes_are_ast_backed() {
        let cases = [
            (
                "fixtures/tier2/reads_writes.js",
                "function run(input) {\n  let value = input;\n  let output = value;\n  return output;\n}\n",
            ),
            (
                "fixtures/tier2/reads_writes.ts",
                "function run(input: string): string {\n  let value = input;\n  let output = value;\n  return output;\n}\n",
            ),
            (
                "fixtures/tier2/reads_writes.jsx",
                "function run(input) {\n  const value = input;\n  const output = value;\n  return <span>{output}</span>;\n}\n",
            ),
            (
                "fixtures/tier2/reads_writes.tsx",
                "function run(input: string) {\n  const value = input;\n  const output = value;\n  return <span>{output}</span>;\n}\n",
            ),
            (
                "fixtures/tier2/reads_writes.py",
                "def run(input):\n    value = input\n    output = value\n    return output\n",
            ),
            (
                "fixtures/tier2/reads_writes.go",
                "package main\n\nfunc run(input int) int {\n    value := input\n    output := value\n    return output\n}\n",
            ),
            (
                "fixtures/tier2/reads_writes.rs",
                "fn run(input: i32) -> i32 {\n    let value = input;\n    let output = value;\n    output\n}\n",
            ),
        ];

        for (path, source) in cases {
            let extraction = extraction(path, source);
            assert_extraction_spans_inside_source(path, source, &extraction);
            let run = extraction
                .entities
                .iter()
                .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
                .unwrap_or_else(|| panic!("{path}: missing run function"));

            assert!(
                extraction.edges.iter().any(|edge| {
                    edge.relation == RelationKind::Writes
                        && edge.head_id == run.id
                        && entity_name(&extraction, &edge.tail_id) == Some("value")
                        && edge.exactness == Exactness::ParserVerified
                }),
                "{path}: missing exact local write to value"
            );
            assert!(
                extraction.edges.iter().any(|edge| {
                    edge.relation == RelationKind::Reads
                        && edge.head_id == run.id
                        && entity_name(&extraction, &edge.tail_id) == Some("input")
                        && edge.exactness == Exactness::ParserVerified
                }),
                "{path}: missing exact parameter read from input"
            );
            assert!(
                extraction.edges.iter().any(|edge| {
                    edge.relation == RelationKind::Reads
                        && edge.head_id == run.id
                        && entity_name(&extraction, &edge.tail_id) == Some("value")
                        && edge.exactness == Exactness::ParserVerified
                }),
                "{path}: missing exact local read from value"
            );
        }
    }

    #[test]
    fn tier2_method_and_computed_calls_stay_heuristic_without_receiver_proof() {
        let source = "\
function helper() { return 1; }
export function run(client: { helper: () => number }, registry: Record<string, Function>, name: string) {
  helper();
  client.helper();
  registry[name]();
}
";
        let extraction = extraction("fixtures/tier2/method_split.ts", source);
        let run = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
            .expect("run function");
        let helper = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Function
                    && entity.name == "helper"
                    && entity.created_from != "tree-sitter-static-heuristic"
            })
            .expect("helper function");

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Calls
                && edge.head_id == run.id
                && edge.tail_id == helper.id
                && edge.exactness == Exactness::ParserVerified
        }));
        let heuristic_calls = extraction
            .edges
            .iter()
            .filter(|edge| {
                edge.relation == RelationKind::Calls
                    && edge.head_id == run.id
                    && edge.exactness == Exactness::StaticHeuristic
            })
            .collect::<Vec<_>>();
        assert!(
            heuristic_calls.len() >= 2,
            "member and computed calls should remain heuristic"
        );
        assert!(heuristic_calls.iter().all(|edge| edge.confidence < 1.0));
    }

    #[test]
    fn tier2_constructor_call_resolves_only_when_class_constructor_is_proven() {
        let source = "\
class Box {
  constructor() {}
}
export function run() {
  return new Box();
}
";
        let extraction = extraction("fixtures/tier2/constructor.ts", source);
        let run = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
            .expect("run function");
        let constructor = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Constructor
                    && entity.name == "constructor"
                    && entity.created_from != "tree-sitter-static-heuristic"
            })
            .expect("constructor declaration");

        let call = extraction
            .edges
            .iter()
            .find(|edge| {
                edge.relation == RelationKind::Calls
                    && edge.head_id == run.id
                    && edge.tail_id == constructor.id
            })
            .expect("constructor CALLS edge");
        assert_eq!(call.exactness, Exactness::ParserVerified);
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Instantiates
                && edge.head_id == run.id
                && entity_name(&extraction, &edge.tail_id) == Some("Box")
                && edge.exactness == Exactness::ParserVerified
        }));
    }

    #[test]
    fn tier2_comments_and_strings_do_not_create_calls_reads_or_writes() {
        let source = "\
export function run() {
  const text = \"target(input)\";
  // target(input)
  return text;
}
";
        let extraction = extraction("fixtures/tier2/comments_strings.ts", source);

        assert!(!extraction.entities.iter().any(|entity| {
            matches!(entity.kind, EntityKind::Function | EntityKind::Method)
                && entity.name.contains("target")
        }));
        assert!(!extraction.edges.iter().any(|edge| {
            matches!(
                entity_name(&extraction, &edge.tail_id),
                Some("target") | Some("input")
            ) && matches!(
                edge.relation,
                RelationKind::Calls | RelationKind::Reads | RelationKind::Writes
            )
        }));
    }

    #[test]
    fn tier2_c_and_cpp_direct_calls_are_supported_when_syntax_is_plain() {
        let cases = [
            (
                "fixtures/tier2/direct.c",
                "int helper(int value) { return value; }\nint run(int input) {\n  return helper(input);\n}\n",
            ),
            (
                "fixtures/tier2/direct.cpp",
                "int helper(int value) { return value; }\nint run(int input) {\n  return helper(input);\n}\n",
            ),
        ];

        for (path, source) in cases {
            let extraction = extraction(path, source);
            let helper = extraction
                .entities
                .iter()
                .find(|entity| {
                    entity.kind == EntityKind::Function
                        && entity.name == "helper"
                        && entity.created_from != "tree-sitter-static-heuristic"
                })
                .unwrap_or_else(|| panic!("{path}: missing helper function"));
            let run = extraction
                .entities
                .iter()
                .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
                .unwrap_or_else(|| panic!("{path}: missing run function"));
            assert!(
                extraction.edges.iter().any(|edge| {
                    edge.relation == RelationKind::Calls
                        && edge.head_id == run.id
                        && edge.tail_id == helper.id
                        && edge.exactness == Exactness::ParserVerified
                }),
                "{path}: missing exact direct C-family call"
            );
        }
    }

    #[test]
    fn extracted_edges_pass_relation_domain_codomain_validation() {
        let extraction = extraction("fixtures/core_relations.ts", CORE_RELATIONS);
        let kinds = extraction
            .entities
            .iter()
            .map(|entity| (entity.id.as_str(), entity.kind))
            .collect::<std::collections::BTreeMap<_, _>>();

        for edge in &extraction.edges {
            let head = kinds
                .get(edge.head_id.as_str())
                .copied()
                .expect("head entity kind");
            let tail = kinds
                .get(edge.tail_id.as_str())
                .copied()
                .expect("tail entity kind");
            assert!(
                codegraph_core::relation_allows(edge.relation, head, tail),
                "invalid edge {:?}: {head:?} -> {tail:?}",
                edge.relation
            );
        }
    }

    #[test]
    fn tier5_async_await_direct_call_adds_exact_awaits_only_for_known_callee() {
        let source = "\
async function loadUser() {
  return 1;
}

export async function run(registry: Record<string, () => Promise<number>>, name: string) {
  await loadUser();
  await registry[name]();
}
";
        let extraction = extraction("fixtures/tier5/async_await.ts", source);
        assert_extraction_spans_inside_source("fixtures/tier5/async_await.ts", source, &extraction);
        let run = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
            .expect("run function");
        let load_user = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "loadUser")
            .expect("loadUser function");

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Awaits
                && edge.head_id == run.id
                && edge.tail_id == load_user.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.start_line == 6
        }));
        assert!(!extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Awaits
                && edge.exactness == Exactness::ParserVerified
                && entity_name(&extraction, &edge.tail_id) == Some("name")
        }));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Awaits && edge.exactness == Exactness::StaticHeuristic
        }));
    }

    #[test]
    fn tier5_promise_direct_call_adds_exact_spawn_but_computed_callback_stays_heuristic() {
        let source = "\
function loadUser() {
  return 1;
}

export function run(registry: Record<string, () => number>, name: string) {
  Promise.all([loadUser()]);
  setTimeout(registry[name], 1);
}
";
        let extraction = extraction("fixtures/tier5/promise_callback.ts", source);
        let run = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
            .expect("run function");
        let load_user = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "loadUser")
            .expect("loadUser function");

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Spawns
                && edge.head_id == run.id
                && edge.tail_id == load_user.id
                && edge.exactness == Exactness::ParserVerified
        }));
        assert!(!extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Handles
                && edge.exactness == Exactness::ParserVerified
                && entity_name(&extraction, &edge.tail_id) == Some("name")
        }));
    }

    #[test]
    fn tier5_event_literal_and_settimeout_named_handler_are_exact_when_handler_is_known() {
        let source = "\
function handleReady(event: unknown) {
  return event;
}

export function start(emitter: { on(name: string, handler: unknown): void }, registry: Record<string, Function>, name: string) {
  emitter.on(\"ready\", handleReady);
  emitter.on(name, handleReady);
  setTimeout(handleReady, 1);
  setTimeout(registry[name], 1);
}
";
        let extraction = extraction("fixtures/tier5/event_callback.ts", source);
        let start = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "start")
            .expect("start function");
        let handler = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "handleReady")
            .expect("handleReady function");

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::ListensTo
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.start_line == 6
        }));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Handles
                && edge.tail_id == handler.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.start_line == 6
        }));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Spawns
                && edge.head_id == start.id
                && edge.tail_id == handler.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.start_line == 8
        }));
        assert!(!extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Handles
                && edge.exactness == Exactness::ParserVerified
                && entity_name(&extraction, &edge.tail_id) == Some("name")
        }));
    }

    #[test]
    fn tier5_literal_route_handler_and_role_check_are_exact_without_comment_proof() {
        let source = "\
function handleAdmin(req: unknown, res: unknown) {
  return req;
}

export function boot(app: any) {
  const text = \"checkRole('admin')\";
  // checkRole('admin')
  checkRole(\"admin\");
  app.get(\"/admin\", handleAdmin);
  app.get(prefix + \"/dynamic\", handleAdmin);
  return text;
}
";
        let extraction = extraction("fixtures/tier5/route_security.ts", source);
        let handler = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "handleAdmin")
            .expect("handleAdmin function");

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Exposes
                && edge.exactness == Exactness::ParserVerified
                && entity_name(&extraction, &edge.head_id) == Some("GET /admin")
        }));
        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Handles
                && edge.tail_id == handler.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.start_line == 9
        }));
        let exact_role_checks = extraction
            .edges
            .iter()
            .filter(|edge| {
                edge.relation == RelationKind::ChecksRole
                    && edge.exactness == Exactness::ParserVerified
                    && entity_name(&extraction, &edge.tail_id) == Some("admin")
            })
            .count();
        assert_eq!(exact_role_checks, 1);
        assert!(!extraction
            .entities
            .iter()
            .any(|entity| entity.name == "GET /dynamic"));
    }

    #[test]
    fn tier5_go_goroutine_direct_call_is_exact_without_channel_dataflow_claim() {
        let source = "\
package main

func worker() {}

func run(ch chan int) {
    go worker()
    ch <- 1
    <-ch
}
";
        let extraction = extraction("fixtures/tier5/goroutine.go", source);
        let run = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "run")
            .expect("run function");
        let worker = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "worker")
            .expect("worker function");

        assert!(extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Spawns
                && edge.head_id == run.id
                && edge.tail_id == worker.id
                && edge.exactness == Exactness::ParserVerified
                && edge.source_span.start_line == 6
        }));
        assert!(!extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::FlowsTo
                && (entity_name(&extraction, &edge.head_id) == Some("ch")
                    || entity_name(&extraction, &edge.tail_id) == Some("ch"))
        }));
    }

    #[test]
    fn tier5_di_service_locator_does_not_emit_exact_injects_without_resolver() {
        let source = "\
export function boot(container: any, impl: unknown, name: string) {
  container.register(\"svc\", impl);
  container.resolve(name);
}
";
        let extraction = extraction("fixtures/tier5/di_unsupported.ts", source);

        assert!(!extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Injects
                && super::is_proof_grade_exactness(edge.exactness)
        }));
    }

    #[test]
    fn tier5_dynamic_import_literal_and_computed_targets_are_not_exact_without_resolver() {
        let source = "\
export async function loadLiteral() {
  return import(\"./plugins/fixed\");
}

export async function loadComputed(name: string) {
  return import(\"./plugins/\" + name);
}

export function loadRequireLiteral() {
  return require(\"./legacy\");
}

export function loadRequireComputed(name: string) {
  return require(name);
}
";
        let extraction = extraction("fixtures/tier5/dynamic_import.ts", source);
        assert_extraction_spans_inside_source(
            "fixtures/tier5/dynamic_import.ts",
            source,
            &extraction,
        );

        let literal_import = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Import
                    && entity.name == "./plugins/fixed"
                    && entity
                        .metadata
                        .get("import_kind")
                        .and_then(serde_json::Value::as_str)
                        == Some("dynamic_import_literal")
            })
            .expect("literal dynamic import artifact");
        assert_eq!(
            literal_import
                .metadata
                .get("target_resolution_claim_state")
                .and_then(serde_json::Value::as_str),
            Some("unsupported")
        );
        assert_eq!(
            literal_import
                .metadata
                .get("module_specifier")
                .and_then(serde_json::Value::as_str),
            Some("./plugins/fixed")
        );

        let computed_import = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Import
                    && entity
                        .metadata
                        .get("import_kind")
                        .and_then(serde_json::Value::as_str)
                        == Some("dynamic_import_computed")
            })
            .expect("computed dynamic import artifact");
        assert_eq!(
            computed_import
                .metadata
                .get("unsupported_reason")
                .and_then(serde_json::Value::as_str),
            Some("computed dynamic module target is unsupported for exact import proof")
        );

        let literal_require = extraction
            .entities
            .iter()
            .find(|entity| {
                entity.kind == EntityKind::Import
                    && entity.name == "./legacy"
                    && entity
                        .metadata
                        .get("import_kind")
                        .and_then(serde_json::Value::as_str)
                        == Some("dynamic_require_literal")
            })
            .expect("literal dynamic require artifact");

        for import_id in [
            literal_import.id.as_str(),
            computed_import.id.as_str(),
            literal_require.id.as_str(),
        ] {
            let import_edge = extraction
                .edges
                .iter()
                .find(|edge| edge.relation == RelationKind::Imports && edge.tail_id == import_id)
                .expect("dynamic module IMPORTS edge");
            assert_eq!(import_edge.exactness, Exactness::StaticHeuristic);
            assert_eq!(
                import_edge
                    .metadata
                    .get("target_resolution_claim_state")
                    .and_then(serde_json::Value::as_str),
                Some("unsupported")
            );
        }
        assert!(!extraction.edges.iter().any(|edge| {
            edge.relation == RelationKind::Imports
                && edge.exactness == Exactness::ParserVerified
                && [
                    literal_import.id.as_str(),
                    computed_import.id.as_str(),
                    literal_require.id.as_str(),
                ]
                .contains(&edge.tail_id.as_str())
        }));
    }

    #[test]
    fn express_route_fixture_produces_exposes() {
        let extraction = extraction("fixtures/route_auth_security.ts", ROUTE_AUTH_SECURITY);

        assert_phase_07_relation(&extraction, RelationKind::Exposes);
    }

    #[test]
    fn role_check_fixture_produces_checks_role() {
        let extraction = extraction("fixtures/route_auth_security.ts", ROUTE_AUTH_SECURITY);

        assert_phase_07_relation(&extraction, RelationKind::ChecksRole);
        assert!(extraction
            .entities
            .iter()
            .any(|entity| entity.kind == EntityKind::Role && entity.name == "admin"));
    }

    #[test]
    fn sanitizer_validator_fixture_produces_sanitizes_and_validates() {
        let extraction = extraction("fixtures/route_auth_security.ts", ROUTE_AUTH_SECURITY);

        assert_phase_07_relation(&extraction, RelationKind::Sanitizes);
        assert_phase_07_relation(&extraction, RelationKind::Validates);
        assert_phase_07_relation(&extraction, RelationKind::SourceOfTaint);
        assert_phase_07_relation(&extraction, RelationKind::SinksTo);
    }

    #[test]
    fn taint_source_entity_uses_span_hash_identity_not_raw_expression_text() {
        let padding = "x".repeat(20_000);
        let source = format!(
            "export function demo(req: any) {{\n  track({{ source: req.body, padding: \"{padding}\" }});\n}}\n"
        );
        let extraction = extraction("fixtures/long_taint_source.ts", &source);
        let taint_edge = extraction
            .edges
            .iter()
            .find(|edge| edge.relation == RelationKind::SourceOfTaint)
            .expect("SOURCE_OF_TAINT edge");
        let taint_source = extraction
            .entities
            .iter()
            .find(|entity| entity.id == taint_edge.head_id)
            .expect("taint source entity");

        assert_eq!(taint_source.kind, EntityKind::Expression);
        assert!(taint_source.name.starts_with("taint_source@"));
        assert!(taint_source.qualified_name.contains("taint_source@"));
        assert!(taint_source.name.len() < 96);
        assert!(taint_source.qualified_name.len() < 160);
        assert!(!taint_source.name.contains("req.body"));
        assert!(!taint_source.qualified_name.contains("req.body"));
        assert!(!taint_source.name.contains(&padding[..256]));
        assert!(!taint_source.qualified_name.contains(&padding[..256]));
        assert_eq!(
            taint_source
                .metadata
                .get("identity_material")
                .and_then(|value| value.as_str()),
            Some("source_span_and_text_hash")
        );
        assert!(taint_source.metadata.contains_key("source_text_hash"));

        let span = taint_source.source_span.as_ref().expect("source span");
        let line = source
            .lines()
            .nth(span.start_line as usize - 1)
            .expect("span line");
        assert!(line.contains("req.body"));
    }

    #[test]
    fn event_emitter_listener_fixture_produces_event_relations() {
        let extraction = extraction("fixtures/events_async.ts", EVENTS_ASYNC);

        assert_phase_07_relation(&extraction, RelationKind::Emits);
        assert_phase_07_relation(&extraction, RelationKind::ListensTo);
        assert_phase_07_relation(&extraction, RelationKind::Publishes);
        assert_phase_07_relation(&extraction, RelationKind::Consumes);
        assert_phase_07_relation(&extraction, RelationKind::SubscribesTo);
        assert_phase_07_relation(&extraction, RelationKind::Handles);
    }

    #[test]
    fn promise_async_fixture_produces_awaits_and_spawns() {
        let extraction = extraction("fixtures/events_async.ts", EVENTS_ASYNC);

        assert_phase_07_relation(&extraction, RelationKind::Awaits);
        assert_phase_07_relation(&extraction, RelationKind::Spawns);
    }

    #[test]
    fn migration_fixture_produces_schema_relations() {
        let extraction = extraction("fixtures/migration.ts", MIGRATION);

        assert_phase_07_relation(&extraction, RelationKind::Migrates);
        assert_phase_07_relation(&extraction, RelationKind::AltersColumn);
        assert_phase_07_relation(&extraction, RelationKind::ReadsTable);
        assert_phase_07_relation(&extraction, RelationKind::WritesTable);
        assert_phase_07_relation(&extraction, RelationKind::DependsOnSchema);
    }

    #[test]
    fn jest_vitest_fixture_produces_test_relations() {
        let extraction = extraction("fixtures/auth.spec.ts", AUTH_SPEC);

        assert_phase_07_relation(&extraction, RelationKind::Tests);
        assert_phase_07_relation(&extraction, RelationKind::Asserts);
        assert_phase_07_relation(&extraction, RelationKind::Mocks);
        assert_phase_07_relation(&extraction, RelationKind::Stubs);
        assert_phase_07_relation(&extraction, RelationKind::Covers);
        assert_phase_07_relation(&extraction, RelationKind::FixturesFor);
    }

    #[test]
    fn all_phase_07_relations_have_fixture_backed_extractors() {
        let extractions = [
            extraction("fixtures/route_auth_security.ts", ROUTE_AUTH_SECURITY),
            extraction("fixtures/events_async.ts", EVENTS_ASYNC),
            extraction("fixtures/migration.ts", MIGRATION),
            extraction("fixtures/auth.spec.ts", AUTH_SPEC),
        ];
        let relations = extractions
            .iter()
            .flat_map(|extraction| extraction.edges.iter().map(|edge| edge.relation))
            .collect::<BTreeSet<_>>();

        for relation in PHASE_07_RELATIONS {
            assert!(relations.contains(relation), "missing {relation}");
        }
    }

    #[test]
    fn store_integration_persists_extracted_entities_and_edges() {
        let store = match SqliteGraphStore::open_in_memory() {
            Ok(store) => store,
            Err(error) => panic!("expected in-memory sqlite store, got {error}"),
        };
        let extraction = extraction("fixtures/simple_function.ts", SIMPLE_FUNCTION);

        if let Err(error) = store.upsert_file(&extraction.file) {
            panic!("expected file upsert, got {error}");
        }
        for entity in &extraction.entities {
            if let Err(error) = store.upsert_entity(entity) {
                panic!("expected entity upsert, got {error}");
            }
            if let Some(span) = &entity.source_span {
                if let Err(error) = store.upsert_source_span(&entity.id, span) {
                    panic!("expected entity source span upsert, got {error}");
                }
            }
        }
        for edge in &extraction.edges {
            if let Err(error) = store.upsert_edge(edge) {
                panic!("expected edge upsert, got {error}");
            }
        }

        let function = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Function && entity.name == "add")
            .expect("add function entity");
        let stored = match store.get_entity(&function.id) {
            Ok(Some(entity)) => entity,
            Ok(None) => panic!("expected stored entity"),
            Err(error) => panic!("expected entity read, got {error}"),
        };
        assert_eq!(stored, *function);

        let module = extraction
            .entities
            .iter()
            .find(|entity| entity.kind == EntityKind::Module)
            .expect("module entity");
        let defined = match store.find_edges_by_head_relation(&module.id, RelationKind::Defines) {
            Ok(edges) => edges,
            Err(error) => panic!("expected edge query, got {error}"),
        };
        assert!(defined.iter().any(|edge| edge.tail_id == function.id));
    }

    fn entity_name<'a>(extraction: &'a super::BasicExtraction, id: &str) -> Option<&'a str> {
        extraction
            .entities
            .iter()
            .find(|entity| entity.id == id)
            .map(|entity| entity.name.as_str())
    }
}
