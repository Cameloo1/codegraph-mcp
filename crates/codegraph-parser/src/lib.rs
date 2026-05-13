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
    relation_allows, stable_edge_id, stable_entity_id, stable_entity_id_for_kind, Edge, EdgeClass,
    EdgeContext, Entity, EntityKind, Exactness, FileRecord, RelationKind, SourceSpan,
};
use serde::{Deserialize, Serialize};
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
        let Some(language) = detect_language(repo_relative_path) else {
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

struct BasicEntityExtractor<'a> {
    parsed: &'a ParsedFile,
    source: &'a str,
    file_hash: String,
    entities: Vec<Entity>,
    edges: Vec<Edge>,
    entity_kinds: BTreeMap<String, EntityKind>,
    edge_ids: BTreeSet<String>,
    scope_parents: BTreeMap<String, String>,
    symbols_by_scope: BTreeMap<String, BTreeMap<String, SymbolRef>>,
    ambiguous_symbols_by_scope: BTreeMap<String, BTreeSet<String>>,
    table_symbols_by_scope: BTreeMap<String, BTreeMap<String, String>>,
    test_file_id: Option<String>,
}

#[derive(Debug, Clone)]
struct SymbolRef {
    id: String,
    exactness: Exactness,
    confidence: f64,
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
    edges: Vec<Edge>,
    entity_kinds: BTreeMap<String, EntityKind>,
    edge_ids: BTreeSet<String>,
    scope_parents: BTreeMap<String, String>,
    symbols_by_scope: BTreeMap<String, BTreeMap<String, SymbolRef>>,
    ambiguous_symbols_by_scope: BTreeMap<String, BTreeSet<String>>,
}

impl<'a> GenericLanguageExtractor<'a> {
    fn new(parsed: &'a ParsedFile, source: &'a str) -> Self {
        Self {
            parsed,
            source,
            file_hash: content_hash(source),
            entities: Vec::new(),
            edges: Vec::new(),
            entity_kinds: BTreeMap::new(),
            edge_ids: BTreeSet::new(),
            scope_parents: BTreeMap::new(),
            symbols_by_scope: BTreeMap::new(),
            ambiguous_symbols_by_scope: BTreeMap::new(),
        }
    }

    fn extract(mut self) -> BasicExtraction {
        let file_record = FileRecord {
            repo_relative_path: self.parsed.repo_relative_path.clone(),
            file_hash: self.file_hash.clone(),
            language: Some(self.parsed.language.to_string()),
            size_bytes: self.parsed.byte_len as u64,
            indexed_at_unix_ms: None,
            metadata: Default::default(),
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

        BasicExtraction {
            file: file_record,
            entities: self.entities,
            edges: self.edges,
        }
    }

    fn visit_children(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_node(child, scope_id, scope_name);
        }
    }

    fn visit_node(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        if let Some(mut kind) = generic_decl_kind(self.parsed.language, node) {
            if let Some(name) = generic_decl_name(node, self.source) {
                if kind == EntityKind::Function
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
                let id = self.push_scoped_entity(scope_id, kind, &name, &qualified_name, node);
                self.extract_parameters(node, &id, &qualified_name);
                self.visit_children(node, &id, &qualified_name);
                return;
            }
        }

        if is_generic_import_node(self.parsed.language, node) {
            self.extract_import(scope_id, scope_name, node);
        } else if is_generic_export_node(self.parsed.language, node, self.source) {
            self.extract_export(scope_id, scope_name, node);
        } else if is_generic_call_node(self.parsed.language, node) {
            self.extract_call(node, scope_id, scope_name);
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

    fn extract_import(&mut self, scope_id: &str, scope_name: &str, node: Node<'_>) {
        let name = generic_import_name(self.parsed.language, node, self.source)
            .unwrap_or_else(|| statement_label(node, self.source));
        let qualified_name = qualify(scope_name, &format!("import:{name}"));
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let id = self.push_entity(EntityKind::Import, &name, &qualified_name, span.clone());
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(scope_id, RelationKind::Imports, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
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
                self.push_scoped_entity(
                    parent_id,
                    EntityKind::Parameter,
                    &name,
                    &qualified_name,
                    child,
                );
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

        if let Some(arguments) = generic_call_arguments_node(self.parsed.language, node) {
            self.extract_call_arguments(arguments, &callsite_id, scope_id, scope_name);
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
        }
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
                0.4,
            );
            return (id, Exactness::StaticHeuristic, 0.4);
        };
        let label = expression_label(callee_node, self.source);
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
        let id = self.push_reference_entity(kind, &label, scope_name, callee_node, 0.55);
        (id, Exactness::StaticHeuristic, 0.55)
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

    fn register_symbol(&mut self, scope_id: &str, name: &str, id: &str) {
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
                exactness: Exactness::ParserVerified,
                confidence: 1.0,
            },
        );
    }

    fn push_reference_entity(
        &mut self,
        kind: EntityKind,
        name: &str,
        scope_name: &str,
        node: Node<'_>,
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
        if !self.entities.iter().any(|entity| entity.id == id) {
            self.entities.push(Entity {
                id: id.clone(),
                kind,
                name: name.to_string(),
                qualified_name,
                repo_relative_path: self.parsed.repo_relative_path.clone(),
                source_span: Some(span),
                content_hash: None,
                file_hash: Some(self.file_hash.clone()),
                created_from: "tree-sitter-static-heuristic".to_string(),
                confidence,
                metadata: Default::default(),
            });
        }
        if let Some(entity) = self.entities.iter_mut().find(|entity| entity.id == id) {
            entity
                .metadata
                .insert("expression_reason".to_string(), "unresolved-callee".into());
            entity.metadata.insert(
                "resolution".to_string(),
                "unresolved_static_heuristic".into(),
            );
            entity.metadata.insert("phase".to_string(), "28".into());
        }
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
        if is_scope_kind(kind) {
            self.scope_parents.insert(id.clone(), scope_id.to_string());
        }
        self.register_symbol(scope_id, name, &id);
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
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
        self.push_edge(scope_id, relation, &id, &span);
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
        if !self.entities.iter().any(|entity| entity.id == id) {
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
            self.entities.push(entity);
        }
        id
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
            context: if is_test_file_path(&span.repo_relative_path) {
                EdgeContext::Test
            } else {
                EdgeContext::Production
            },
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
            edges: Vec::new(),
            entity_kinds: BTreeMap::new(),
            edge_ids: BTreeSet::new(),
            scope_parents: BTreeMap::new(),
            symbols_by_scope: BTreeMap::new(),
            ambiguous_symbols_by_scope: BTreeMap::new(),
            table_symbols_by_scope: BTreeMap::new(),
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
            metadata: Default::default(),
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

    fn visit_node(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
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
                    let span = source_span_for_node(&self.parsed.repo_relative_path, node);
                    self.push_edge(scope_id, RelationKind::Writes, &target_id, &span);
                    if looks_like_table_constant_name(&name) {
                        self.push_table_constant_entity(scope_id, &name, &qualified_name, node);
                    }
                    if let Some(value) = node.child_by_field_name("value") {
                        if let Some(source_id) = self.expression_entity(value, scope_id, scope_name)
                        {
                            let value_span =
                                source_span_for_node(&self.parsed.repo_relative_path, value);
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
                        self.extract_reads_from_expression(value, scope_id, Some(&target_id));
                    }
                }
            }
            "assignment_expression" | "augmented_assignment_expression" => {
                self.extract_assignment(node, scope_id, scope_name);
            }
            "call_expression" => {
                self.extract_call(node, scope_id, scope_name);
            }
            "new_expression" => {
                self.extract_new_expression(node, scope_id, scope_name);
            }
            "await_expression" => {
                self.extract_await_expression(node, scope_id, scope_name);
            }
            "return_statement" => {
                self.extract_return(node, scope_id, scope_name);
            }
            "import_statement" => {
                let name = statement_label(node, self.source);
                let qualified_name = qualify(scope_name, &name);
                let span = source_span_for_node(&self.parsed.repo_relative_path, node);
                let id = self.push_entity(EntityKind::Import, &name, &qualified_name, span.clone());
                self.push_edge(scope_id, RelationKind::Contains, &id, &span);
                self.push_edge(scope_id, RelationKind::Imports, &id, &span);
                self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
            }
            "export_statement" => {
                let name = statement_label(node, self.source);
                let qualified_name = qualify(scope_name, &name);
                let span = source_span_for_node(&self.parsed.repo_relative_path, node);
                let id = self.push_entity(EntityKind::Export, &name, &qualified_name, span.clone());
                self.push_edge(scope_id, RelationKind::Contains, &id, &span);
                self.push_edge(scope_id, RelationKind::Exports, &id, &span);
                self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
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
            }
            _ => {}
        }

        self.visit_children(node, scope_id, scope_name);
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
                self.push_scoped_entity(
                    parent_id,
                    EntityKind::Parameter,
                    &name,
                    &qualified_name,
                    parameter,
                );
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
        let Some(target_id) = self.assignment_target_entity(left, scope_id, scope_name) else {
            return;
        };
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        self.push_edge(scope_id, RelationKind::Writes, &target_id, &span);
        if left.kind() == "member_expression" || left.kind() == "subscript_expression" {
            self.push_edge(scope_id, RelationKind::Mutates, &target_id, &span);
        }

        if let Some(source_id) = self.expression_entity(right, scope_id, scope_name) {
            self.push_edge(&target_id, RelationKind::AssignedFrom, &source_id, &span);
            self.push_edge(&source_id, RelationKind::FlowsTo, &target_id, &span);
        }
        self.extract_reads_from_expression(right, scope_id, Some(&target_id));
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

        if let Some(arguments) = node.child_by_field_name("arguments") {
            self.extract_call_arguments(arguments, &callsite_id, scope_id, scope_name);
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

    fn extract_extended_call_relations(
        &mut self,
        node: Node<'_>,
        callsite_id: &str,
        scope_id: &str,
        scope_name: &str,
        callee_label: &str,
    ) {
        let arguments = call_argument_nodes(node);
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
        self.push_extended_edge(
            &route_id,
            RelationKind::Exposes,
            &endpoint_id,
            &span,
            HeuristicTag::new("express-route", "express", 0.72),
        );

        for argument in arguments.iter().skip(1) {
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
            let role =
                first_string_argument(arguments, self.source).unwrap_or_else(|| "role".to_string());
            let role_id = self.push_extended_entity(
                EntityKind::Role,
                &role,
                scope_name,
                node,
                HeuristicTag::new("role-check-call", "auth", 0.66),
            );
            self.push_extended_edge(
                callsite_id,
                RelationKind::ChecksRole,
                &role_id,
                &span,
                HeuristicTag::new("role-check-call", "auth", 0.66),
            );
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
        let channel = first_string_argument(arguments, self.source)
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
            self.push_extended_edge(
                callsite_id,
                RelationKind::ListensTo,
                &event_id,
                &span,
                HeuristicTag::new("event-listener-call", "event-emitter", 0.64),
            );
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

    fn extract_await_expression(&mut self, node: Node<'_>, scope_id: &str, scope_name: &str) {
        let span = source_span_for_node(&self.parsed.repo_relative_path, node);
        let label = expression_label(node, self.source);
        let head_id = self.executable_or_task_head(scope_id, scope_name, node);
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

        if let Some(value) = first_return_value(node) {
            if let Some(source_id) = self.expression_entity(value, scope_id, scope_name) {
                let value_span = source_span_for_node(&self.parsed.repo_relative_path, value);
                self.push_edge(&source_id, RelationKind::FlowsTo, &return_id, &value_span);
            }
            self.extract_reads_from_expression(value, scope_id, None);
            self.extract_table_sink_return(value, scope_id, scope_name);
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
        collect_identifier_nodes(node, &mut identifiers);
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
                        "unresolved-write-target",
                        0.55,
                    ))
                });
        }

        Some(self.push_expression_entity(
            &expression_label(node, self.source),
            scope_name,
            node,
            "assignment-target",
            0.85,
        ))
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

    fn register_symbol(&mut self, scope_id: &str, name: &str, id: &str) {
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
                exactness: Exactness::ParserVerified,
                confidence: 1.0,
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
        if is_scope_kind(kind) {
            self.scope_parents.insert(id.clone(), scope_id.to_string());
        }
        self.register_symbol(scope_id, name, &id);
        self.push_edge(scope_id, RelationKind::Contains, &id, &span);
        self.push_edge(&id, RelationKind::DefinedIn, scope_id, &span);
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
        self.push_edge(scope_id, declaration_relation, &id, &span);
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
        if !self.entities.iter().any(|entity| entity.id == id) {
            self.entities.push(Entity {
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
            });
        }
        id
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
            context: if is_test_file_path(&span.repo_relative_path) {
                EdgeContext::Test
            } else {
                EdgeContext::Production
            },
            derived: false,
            provenance_edges: Vec::new(),
            metadata: Default::default(),
        });
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

fn is_test_file_path(path: &str) -> bool {
    let normalized = normalize_repo_relative_path(path).to_ascii_lowercase();
    normalized.ends_with(".test.ts")
        || normalized.ends_with(".test.tsx")
        || normalized.ends_with(".test.js")
        || normalized.ends_with(".test.jsx")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".spec.js")
        || normalized.ends_with(".spec.jsx")
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
    matches!(kind, EntityKind::Function | EntityKind::Method)
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

fn collect_identifier_nodes<'a>(node: Node<'a>, identifiers: &mut Vec<Node<'a>>) {
    if node.kind() == "identifier" {
        identifiers.push(node);
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
        collect_identifier_nodes(child, identifiers);
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

fn parameter_name(node: Node<'_>, source: &str) -> Option<String> {
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
    )
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
        _ => false,
    }
}

fn generic_call_callee_node(language: SourceLanguage, node: Node<'_>) -> Option<Node<'_>> {
    match language {
        SourceLanguage::Python => node
            .child_by_field_name("function")
            .or_else(|| first_named_child(node)),
        SourceLanguage::Go => node
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
        SourceLanguage::Python | SourceLanguage::Go => node.child_by_field_name("arguments"),
        SourceLanguage::Rust => node
            .child_by_field_name("arguments")
            .or_else(|| child_by_kind(node, "arguments")),
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
        "class_declaration" | "class_definition" | "class" => Some(EntityKind::Class),
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
        collections::BTreeSet,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use codegraph_core::{EntityKind, Exactness, RelationKind};
    use codegraph_store::{GraphStore, SqliteGraphStore};

    use super::{
        detect_language, extract_basic_entities, LanguageFrontend, LanguageParser, SourceLanguage,
        TreeSitterParser,
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

    fn assert_phase_07_relation(extraction: &super::BasicExtraction, relation: RelationKind) {
        let matching = extraction
            .edges
            .iter()
            .filter(|edge| edge.relation == relation)
            .collect::<Vec<_>>();
        assert!(!matching.is_empty(), "missing {relation}");
        for edge in matching {
            assert_eq!(edge.exactness, Exactness::StaticHeuristic, "{relation}");
            assert!(edge.confidence < 1.0, "{relation}");
            assert_eq!(edge.extractor, "tree-sitter-extended-heuristic");
            assert_eq!(
                edge.metadata.get("phase").and_then(|value| value.as_str()),
                Some("07")
            );
            assert!(edge.metadata.contains_key("pattern"), "{relation}");
            assert!(edge.metadata.contains_key("framework"), "{relation}");
            assert!(
                !edge.source_span.repo_relative_path.is_empty(),
                "{relation}"
            );
        }
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
            assert!(extraction.edges.iter().all(|edge| {
                edge.exactness == Exactness::ParserVerified
                    || (edge.relation == RelationKind::Calls
                        && edge.exactness == Exactness::StaticHeuristic)
                    || (edge.relation == RelationKind::Callee
                        && edge.exactness == Exactness::StaticHeuristic)
            }));
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
    fn unresolved_calls_are_marked_static_heuristic() {
        let source = "export function demo(input: string) {\n  return missingCall(input);\n}\n";
        let extraction = extraction("fixtures/unresolved_call.ts", source);

        assert!(extraction
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Calls
                && edge.exactness == codegraph_core::Exactness::StaticHeuristic
                && edge.confidence < 1.0));
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
