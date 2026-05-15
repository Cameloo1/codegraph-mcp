//! Central index-scope policy.
//!
//! The production indexer uses this module for safe default hard excludes.
//! Soft exclude names are still warning-first: they are excluded only when
//! `.gitignore` says so or when they match a known generated artifact prefix.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

pub const SCOPE_EXAMPLE_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopePathKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeAction {
    WouldExclude,
    WouldInclude,
    WouldIncludeWithWarning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeRuleKind {
    ExplicitExclude,
    ExplicitInclude,
    HardExclude,
    SoftExclude,
    Gitignore,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeClassification {
    DefinitelyGeneratedDependencyArtifact,
    LikelyGeneratedDependencyArtifact,
    Ambiguous,
    LikelyLegitimateSource,
    ExplicitlyIncludedByConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeWarning {
    LikelySourceFileInsideWouldExcludedDir,
    SourceBearingDirectoryAffected,
    TestsFixturesExamplesDocsAffected,
    GeneratedDirectoryNotGitignored,
    SourceExtensionInsideSoftExcludePath,
    ExcludedOnlyBecauseBroadPattern,
    SoftExcludeIncludedByDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexScopeOptions {
    pub include_ignored: bool,
    pub no_default_excludes: bool,
    pub respect_gitignore: bool,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub explain_scope: bool,
    pub print_included: bool,
    pub print_excluded: bool,
}

impl Default for IndexScopeOptions {
    fn default() -> Self {
        Self {
            include_ignored: false,
            no_default_excludes: false,
            respect_gitignore: true,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            explain_scope: false,
            print_included: false,
            print_excluded: false,
        }
    }
}

impl IndexScopeOptions {
    pub fn has_include_patterns(&self) -> bool {
        !self.include_patterns.is_empty()
    }

    pub fn has_print_or_explain(&self) -> bool {
        self.explain_scope || self.print_included || self.print_excluded
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexScopeDecision {
    pub normalized_path: String,
    pub path_kind: ScopePathKind,
    pub action: ScopeAction,
    pub rule_kind: ScopeRuleKind,
    pub matched_rule: Option<String>,
    pub reason: String,
    pub classification: ScopeClassification,
    pub gitignored: bool,
    pub warnings: Vec<ScopeWarning>,
}

impl IndexScopeDecision {
    pub fn excluded(&self) -> bool {
        self.action == ScopeAction::WouldExclude
    }

    pub fn warned(&self) -> bool {
        !self.warnings.is_empty() || self.action == ScopeAction::WouldIncludeWithWarning
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexScope {
    options: IndexScopeOptions,
    gitignore: GitIgnoreMatcher,
}

impl IndexScope {
    pub fn new(options: IndexScopeOptions) -> Self {
        Self {
            options,
            gitignore: GitIgnoreMatcher::default(),
        }
    }

    pub fn for_repo(root: &Path, options: IndexScopeOptions) -> Self {
        let gitignore = if options.respect_gitignore {
            GitIgnoreMatcher::from_repo(root)
        } else {
            GitIgnoreMatcher::default()
        };
        Self { options, gitignore }
    }

    pub fn options(&self) -> &IndexScopeOptions {
        &self.options
    }

    pub fn evaluate_repo_path(
        &self,
        path: impl AsRef<str>,
        path_kind: ScopePathKind,
    ) -> IndexScopeDecision {
        let normalized = normalize_scope_path(path.as_ref());
        let gitignored = self.gitignore.is_ignored(&normalized, path_kind);
        self.evaluate_path_with_gitignored(normalized, path_kind, gitignored)
    }

    pub fn evaluate_path(
        &self,
        path: impl AsRef<str>,
        path_kind: ScopePathKind,
        gitignored: bool,
    ) -> IndexScopeDecision {
        self.evaluate_path_with_gitignored(
            normalize_scope_path(path.as_ref()),
            path_kind,
            gitignored,
        )
    }

    fn evaluate_path_with_gitignored(
        &self,
        normalized_path: String,
        path_kind: ScopePathKind,
        gitignored: bool,
    ) -> IndexScopeDecision {
        let components = path_components(&normalized_path);
        let source_file =
            path_kind == ScopePathKind::File && has_source_extension(&normalized_path);
        let source_bearing_component = has_source_bearing_component(&components);
        let tests_docs_component = has_tests_fixtures_examples_docs_component(&components);

        if let Some(pattern) = matching_pattern(&self.options.exclude_patterns, &normalized_path) {
            let mut warnings =
                source_warnings(source_file, source_bearing_component, tests_docs_component);
            if source_file {
                warnings.push(ScopeWarning::LikelySourceFileInsideWouldExcludedDir);
            }
            return IndexScopeDecision {
                normalized_path,
                path_kind,
                action: ScopeAction::WouldExclude,
                rule_kind: ScopeRuleKind::ExplicitExclude,
                matched_rule: Some(format!("explicit_exclude:{pattern}")),
                reason: "path matched an explicit --exclude override".to_string(),
                classification: classify_path(source_bearing_component, false, true),
                gitignored,
                warnings,
            };
        }

        if let Some(pattern) = matching_pattern(&self.options.include_patterns, &normalized_path) {
            return IndexScopeDecision {
                normalized_path,
                path_kind,
                action: ScopeAction::WouldInclude,
                rule_kind: ScopeRuleKind::ExplicitInclude,
                matched_rule: Some(format!("explicit_include:{pattern}")),
                reason: "path matched an explicit --include override".to_string(),
                classification: ScopeClassification::ExplicitlyIncludedByConfig,
                gitignored,
                warnings: Vec::new(),
            };
        }

        if self.options.no_default_excludes {
            return IndexScopeDecision {
                normalized_path,
                path_kind,
                action: ScopeAction::WouldInclude,
                rule_kind: ScopeRuleKind::None,
                matched_rule: Some("no_default_excludes".to_string()),
                reason: "--no-default-excludes disabled default scope filtering".to_string(),
                classification: classify_path(source_bearing_component, false, false),
                gitignored,
                warnings: Vec::new(),
            };
        }

        if let Some(rule) = hard_exclude_rule(&normalized_path, &components, path_kind) {
            let mut warnings =
                source_warnings(source_file, source_bearing_component, tests_docs_component);
            if source_file {
                warnings.push(ScopeWarning::LikelySourceFileInsideWouldExcludedDir);
            }
            return IndexScopeDecision {
                normalized_path,
                path_kind,
                action: ScopeAction::WouldExclude,
                rule_kind: ScopeRuleKind::HardExclude,
                matched_rule: Some(rule),
                reason: "safe hard exclude matched the default index policy".to_string(),
                classification: ScopeClassification::DefinitelyGeneratedDependencyArtifact,
                gitignored,
                warnings,
            };
        }

        if let Some(rule) = soft_exclude_rule(
            &normalized_path,
            &components,
            gitignored,
            self.options.respect_gitignore && !self.options.include_ignored,
        ) {
            let mut warnings =
                source_warnings(source_file, source_bearing_component, tests_docs_component);
            if source_file {
                warnings.push(ScopeWarning::SourceExtensionInsideSoftExcludePath);
            }
            let would_exclude = rule.starts_with("soft_gitignore:")
                || rule.starts_with("soft_known_generated_artifact:");
            if would_exclude && source_file {
                warnings.push(ScopeWarning::LikelySourceFileInsideWouldExcludedDir);
            }
            if !would_exclude {
                warnings.push(ScopeWarning::SoftExcludeIncludedByDefault);
                if rule == "soft_ambiguous:generated" {
                    warnings.push(ScopeWarning::GeneratedDirectoryNotGitignored);
                }
            }
            let action = if would_exclude {
                ScopeAction::WouldExclude
            } else {
                ScopeAction::WouldIncludeWithWarning
            };
            return IndexScopeDecision {
                normalized_path,
                path_kind,
                action,
                rule_kind: ScopeRuleKind::SoftExclude,
                matched_rule: Some(rule),
                reason: if would_exclude {
                    "soft exclude candidate excluded because it is gitignored or a known generated artifact"
                } else {
                    "soft exclude candidate kept by default and reported as a warning"
                }
                .to_string(),
                classification: if would_exclude {
                    ScopeClassification::LikelyGeneratedDependencyArtifact
                } else {
                    ScopeClassification::Ambiguous
                },
                gitignored,
                warnings,
            };
        }

        if self.options.respect_gitignore && !self.options.include_ignored && gitignored {
            let mut warnings =
                source_warnings(source_file, source_bearing_component, tests_docs_component);
            if source_file {
                warnings.push(ScopeWarning::LikelySourceFileInsideWouldExcludedDir);
            }
            return IndexScopeDecision {
                normalized_path,
                path_kind,
                action: ScopeAction::WouldExclude,
                rule_kind: ScopeRuleKind::Gitignore,
                matched_rule: Some("gitignore".to_string()),
                reason: "path is ignored by .gitignore and --respect-gitignore is enabled"
                    .to_string(),
                classification: classify_path(source_bearing_component, true, true),
                gitignored,
                warnings,
            };
        }

        IndexScopeDecision {
            normalized_path,
            path_kind,
            action: ScopeAction::WouldInclude,
            rule_kind: ScopeRuleKind::None,
            matched_rule: None,
            reason: "no default exclude rule matched".to_string(),
            classification: classify_path(source_bearing_component, false, false),
            gitignored,
            warnings: Vec::new(),
        }
    }
}

impl Default for IndexScope {
    fn default() -> Self {
        Self::new(IndexScopeOptions::default())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct IndexScopeRuntimeReport {
    pub default_excludes_enabled: bool,
    pub include_ignored: bool,
    pub no_default_excludes: bool,
    pub respect_gitignore: bool,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub paths_evaluated: usize,
    pub files_included: usize,
    pub paths_excluded: usize,
    pub files_excluded: usize,
    pub warnings: usize,
    pub included_examples: Vec<IndexScopeDecision>,
    pub excluded_examples: Vec<IndexScopeDecision>,
    pub warning_examples: Vec<IndexScopeDecision>,
}

impl IndexScopeRuntimeReport {
    pub fn new(options: &IndexScopeOptions) -> Self {
        Self {
            default_excludes_enabled: !options.no_default_excludes,
            include_ignored: options.include_ignored,
            no_default_excludes: options.no_default_excludes,
            respect_gitignore: options.respect_gitignore,
            include_patterns: options.include_patterns.clone(),
            exclude_patterns: options.exclude_patterns.clone(),
            ..Self::default()
        }
    }

    pub fn record(&mut self, decision: &IndexScopeDecision) {
        self.paths_evaluated += 1;
        if decision.excluded() {
            self.paths_excluded += 1;
            if decision.path_kind == ScopePathKind::File {
                self.files_excluded += 1;
            }
            push_limited(&mut self.excluded_examples, decision.clone());
        } else if decision.path_kind == ScopePathKind::File {
            self.files_included += 1;
            push_limited(&mut self.included_examples, decision.clone());
        }
        if decision.warned() {
            self.warnings += 1;
            push_limited(&mut self.warning_examples, decision.clone());
        }
    }
}

fn push_limited(items: &mut Vec<IndexScopeDecision>, item: IndexScopeDecision) {
    if items.len() < SCOPE_EXAMPLE_LIMIT {
        items.push(item);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GitIgnoreMatcher {
    patterns: Vec<GitIgnorePattern>,
}

impl GitIgnoreMatcher {
    fn from_repo(root: &Path) -> Self {
        let gitignore_path = root.join(".gitignore");
        let Ok(text) = fs::read_to_string(gitignore_path) else {
            return Self::default();
        };
        let patterns = text
            .lines()
            .filter_map(GitIgnorePattern::parse)
            .collect::<Vec<_>>();
        Self { patterns }
    }

    fn is_ignored(&self, path: &str, path_kind: ScopePathKind) -> bool {
        let normalized = normalize_scope_path(path);
        let mut ignored = false;
        for pattern in &self.patterns {
            if pattern.matches(&normalized, path_kind) {
                ignored = !pattern.negated;
            }
        }
        ignored
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitIgnorePattern {
    raw: String,
    pattern: String,
    negated: bool,
    directory_only: bool,
    anchored: bool,
}

impl GitIgnorePattern {
    fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        let (negated, body) = trimmed
            .strip_prefix('!')
            .map(|rest| (true, rest))
            .unwrap_or((false, trimmed));
        let directory_only = body.ends_with('/');
        let body = body.trim_end_matches('/');
        let anchored = body.starts_with('/');
        let pattern = normalize_scope_path(body.trim_start_matches('/'));
        if pattern == "." {
            return None;
        }
        Some(Self {
            raw: trimmed.to_string(),
            pattern,
            negated,
            directory_only,
            anchored,
        })
    }

    fn matches(&self, path: &str, path_kind: ScopePathKind) -> bool {
        if self.directory_only && path_kind == ScopePathKind::File {
            return path_starts_with(path, &self.pattern)
                || path_components(path)
                    .iter()
                    .any(|component| wildcard_match(&self.pattern, component));
        }
        if self.anchored {
            return wildcard_match(&self.pattern, path) || path_starts_with(path, &self.pattern);
        }
        if self.pattern.contains('/') {
            return wildcard_match(&self.pattern, path)
                || path_starts_with(path, &self.pattern)
                || path.contains(&format!("/{}", self.pattern));
        }
        path_components(path)
            .iter()
            .any(|component| wildcard_match(&self.pattern, component))
            || wildcard_match(&self.pattern, path)
    }
}

pub fn normalize_scope_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

fn path_components(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect()
}

fn hard_exclude_rule(
    normalized_path: &str,
    components: &[&str],
    path_kind: ScopePathKind,
) -> Option<String> {
    if path_kind == ScopePathKind::File {
        let lower = normalized_path.to_ascii_lowercase();
        for suffix in HARD_EXCLUDE_FILE_SUFFIXES {
            if lower.ends_with(suffix) {
                return Some(format!("hard_file_suffix:{suffix}"));
            }
        }
    }

    for component in HARD_EXCLUDE_DIR_NAMES {
        if components.iter().any(|candidate| candidate == component) {
            return Some(format!("hard_dir:{component}"));
        }
    }

    for prefix in HARD_EXCLUDE_PREFIXES {
        if path_starts_with(normalized_path, prefix) {
            return Some(format!("hard_prefix:{prefix}"));
        }
    }

    None
}

fn soft_exclude_rule(
    normalized_path: &str,
    components: &[&str],
    gitignored: bool,
    respect_gitignore: bool,
) -> Option<String> {
    let soft_component = SOFT_EXCLUDE_DIR_NAMES
        .iter()
        .find(|component| components.iter().any(|candidate| candidate == *component))?;

    if respect_gitignore && gitignored {
        return Some(format!("soft_gitignore:{soft_component}"));
    }
    if soft_known_generated_artifact(normalized_path) {
        return Some(format!("soft_known_generated_artifact:{soft_component}"));
    }
    Some(format!("soft_ambiguous:{soft_component}"))
}

fn matching_pattern<'a>(patterns: &'a [String], path: &str) -> Option<&'a str> {
    patterns
        .iter()
        .find(|pattern| pattern_matches_path(pattern, path))
        .map(String::as_str)
}

pub fn pattern_matches_path(pattern: &str, path: &str) -> bool {
    let pattern = normalize_scope_path(pattern);
    let path = normalize_scope_path(path);
    if pattern == "." {
        return false;
    }
    if pattern.ends_with("/**") {
        let prefix = pattern.trim_end_matches("/**");
        return path_starts_with(&path, prefix);
    }
    if pattern.contains('*') {
        return wildcard_match(&pattern, &path);
    }
    path == pattern || path_starts_with(&path, &pattern)
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == text;
    }
    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');
    let parts = pattern
        .split('*')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut remainder = text;
    for (index, part) in parts.iter().enumerate() {
        let Some(position) = remainder.find(part) else {
            return false;
        };
        if index == 0 && !starts_with_wildcard && position != 0 {
            return false;
        }
        let next = position + part.len();
        remainder = &remainder[next..];
    }
    if !ends_with_wildcard {
        if let Some(last) = parts.last() {
            return text.ends_with(last);
        }
    }
    true
}

fn path_starts_with(path: &str, prefix: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

fn soft_known_generated_artifact(path: &str) -> bool {
    SOFT_KNOWN_GENERATED_PREFIXES
        .iter()
        .any(|prefix| path_starts_with(path, prefix))
        || path == "dist/bundle.js"
        || path.ends_with("/dist/bundle.js")
        || path == "build/generated.py"
        || path.ends_with("/build/generated.py")
}

fn has_source_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    SOURCE_EXTENSIONS
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn has_source_bearing_component(components: &[&str]) -> bool {
    components
        .iter()
        .any(|component| SOURCE_BEARING_DIR_NAMES.contains(component))
}

fn has_tests_fixtures_examples_docs_component(components: &[&str]) -> bool {
    components
        .iter()
        .any(|component| TESTS_FIXTURES_EXAMPLES_DOCS.contains(component))
}

fn source_warnings(
    source_file: bool,
    source_bearing_component: bool,
    tests_docs_component: bool,
) -> Vec<ScopeWarning> {
    let mut warnings = Vec::new();
    if source_file && source_bearing_component {
        warnings.push(ScopeWarning::SourceBearingDirectoryAffected);
    }
    if source_file && tests_docs_component {
        warnings.push(ScopeWarning::TestsFixturesExamplesDocsAffected);
    }
    warnings
}

fn classify_path(
    source_bearing_component: bool,
    gitignored: bool,
    excluded: bool,
) -> ScopeClassification {
    if gitignored && !excluded {
        ScopeClassification::ExplicitlyIncludedByConfig
    } else if source_bearing_component {
        ScopeClassification::LikelyLegitimateSource
    } else if excluded {
        ScopeClassification::LikelyGeneratedDependencyArtifact
    } else {
        ScopeClassification::Ambiguous
    }
}

const HARD_EXCLUDE_DIR_NAMES: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    ".cache",
    "coverage",
    ".codegraph",
];

const HARD_EXCLUDE_PREFIXES: &[&str] = &[
    ".codegraph-index",
    ".codegraph-competitors",
    ".codegraph-bench-cache",
    ".codegraphcontext",
    ".arl",
    ".tools",
    ".codex-tools",
    "reports/audit/artifacts",
    "reports/final",
    "reports/final/artifacts",
    "reports/comparison/cgc_full_run",
    "reports/comparison/cgc_recovery",
    "reports/comparison/cgc_fork_pr_readiness",
    "reports/comparison/artifacts",
    "reports/comparison/fixtures",
    "reports/cgc-comparison",
    "reports/diagnostic_lab",
    "reports/smoke",
    "target/codegraph-bench-report",
    "crates/codegraph-bench/competitors",
];

const HARD_EXCLUDE_FILE_SUFFIXES: &[&str] = &[
    ".sqlite",
    ".sqlite3",
    ".db",
    ".db-wal",
    ".db-shm",
    ".sqlite-wal",
    ".sqlite-shm",
    ".sqlite3-wal",
    ".sqlite3-shm",
    ".log",
];

const SOFT_EXCLUDE_DIR_NAMES: &[&str] = &[
    "dist",
    "build",
    "out",
    ".next",
    ".turbo",
    "vendor",
    "third_party",
    "generated",
];

const SOFT_KNOWN_GENERATED_PREFIXES: &[&str] = &[
    "codegraph-ui/dist",
    "codegraph-ui/build",
    "codegraph-ui/.next",
    "codegraph-ui/.turbo",
];

const SOURCE_BEARING_DIR_NAMES: &[&str] = &[
    "tests", "fixtures", "examples", "src", "crates", "packages", "apps", "libs", "docs",
];

const TESTS_FIXTURES_EXAMPLES_DOCS: &[&str] = &["tests", "fixtures", "examples", "docs"];

const SOURCE_EXTENSIONS: &[&str] = &[
    ".js", ".mjs", ".cjs", ".jsx", ".ts", ".mts", ".cts", ".tsx", ".py", ".go", ".rs", ".java",
    ".cs", ".c", ".h", ".cc", ".cpp", ".cxx", ".hpp", ".hh", ".hxx", ".rb", ".php",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn default_scope() -> IndexScope {
        IndexScope::new(IndexScopeOptions::default())
    }

    fn decision(path: &str) -> IndexScopeDecision {
        default_scope().evaluate_path(path, ScopePathKind::File, false)
    }

    #[test]
    fn hard_exclude_candidates_would_exclude_in_default_policy() {
        for path in [
            "target/debug/app.rs",
            "node_modules/pkg/index.ts",
            "reports/diagnostic_lab/out.py",
            "reports/final/fake_report.rs",
            "reports/final/artifacts/proof.sqlite",
            "src/cache.sqlite",
            "src/cache.db",
            "src/cache.db-wal",
            "src/cache.db-shm",
            "logs/run.log",
        ] {
            let observed = decision(path);
            assert_eq!(observed.action, ScopeAction::WouldExclude, "{path}");
            assert_eq!(observed.rule_kind, ScopeRuleKind::HardExclude, "{path}");
        }
    }

    #[test]
    fn broad_reports_root_is_not_hard_excluded() {
        let observed = decision("reports/handwritten/source.ts");
        assert_ne!(observed.action, ScopeAction::WouldExclude);
        assert_ne!(observed.rule_kind, ScopeRuleKind::HardExclude);
    }

    #[test]
    fn source_roots_are_not_hard_excluded_by_name() {
        for path in [
            "tests/login.test.ts",
            "fixtures/basic_repo/src/main.py",
            "examples/quickstart.rs",
            "docs/snippets/example.ts",
            "src/lib.rs",
            "crates/codegraph-index/src/lib.rs",
            "packages/web/src/index.ts",
            "apps/console/src/main.ts",
            "libs/shared/src/index.ts",
        ] {
            let observed = decision(path);
            assert_ne!(observed.action, ScopeAction::WouldExclude, "{path}");
            assert_ne!(observed.rule_kind, ScopeRuleKind::HardExclude, "{path}");
        }
    }

    #[test]
    fn soft_excludes_warn_when_not_gitignored() {
        for path in [
            "dist/app.ts",
            "build/source.py",
            "out/main.rs",
            "vendor/library/src/index.ts",
            "third_party/lib/main.py",
            "generated/service.go",
        ] {
            let observed = decision(path);
            assert_eq!(
                observed.action,
                ScopeAction::WouldIncludeWithWarning,
                "{path}"
            );
            assert_eq!(observed.rule_kind, ScopeRuleKind::SoftExclude, "{path}");
            assert!(
                observed
                    .warnings
                    .contains(&ScopeWarning::SourceExtensionInsideSoftExcludePath),
                "{path}"
            );
        }
    }

    #[test]
    fn soft_excludes_would_exclude_when_gitignored_and_respected() {
        let observed = default_scope().evaluate_path("dist/app.ts", ScopePathKind::File, true);
        assert_eq!(observed.action, ScopeAction::WouldExclude);
        assert_eq!(
            observed.matched_rule.as_deref(),
            Some("soft_gitignore:dist")
        );
    }

    #[test]
    fn soft_known_generated_artifacts_are_excluded_without_broadening_soft_dirs() {
        for (path, rule) in [
            ("dist/bundle.js", "soft_known_generated_artifact:dist"),
            ("build/generated.py", "soft_known_generated_artifact:build"),
        ] {
            let observed = default_scope().evaluate_path(path, ScopePathKind::File, false);
            assert_eq!(observed.action, ScopeAction::WouldExclude, "{path}");
            assert_eq!(observed.rule_kind, ScopeRuleKind::SoftExclude, "{path}");
            assert_eq!(observed.matched_rule.as_deref(), Some(rule), "{path}");
        }

        for path in ["dist/source.ts", "build/source.py"] {
            let observed = default_scope().evaluate_path(path, ScopePathKind::File, false);
            assert_eq!(
                observed.action,
                ScopeAction::WouldIncludeWithWarning,
                "{path}"
            );
        }
    }

    #[test]
    fn include_ignored_keeps_gitignored_soft_paths_but_still_warns() {
        let scope = IndexScope::new(IndexScopeOptions {
            include_ignored: true,
            ..IndexScopeOptions::default()
        });
        let observed = scope.evaluate_path("dist/app.ts", ScopePathKind::File, true);
        assert_eq!(observed.action, ScopeAction::WouldIncludeWithWarning);
        assert_eq!(
            observed.matched_rule.as_deref(),
            Some("soft_ambiguous:dist")
        );
    }

    #[test]
    fn no_default_excludes_includes_normally_excluded_paths() {
        let scope = IndexScope::new(IndexScopeOptions {
            no_default_excludes: true,
            ..IndexScopeOptions::default()
        });
        let observed = scope.evaluate_path("target/debug/app.rs", ScopePathKind::File, false);
        assert_eq!(observed.action, ScopeAction::WouldInclude);
        assert_eq!(
            observed.matched_rule.as_deref(),
            Some("no_default_excludes")
        );
    }

    #[test]
    fn explicit_include_overrides_default_hard_exclude() {
        let scope = IndexScope::new(IndexScopeOptions {
            include_patterns: vec!["target/debug/app.rs".to_string()],
            ..IndexScopeOptions::default()
        });
        let observed = scope.evaluate_path("target/debug/app.rs", ScopePathKind::File, false);
        assert_eq!(observed.action, ScopeAction::WouldInclude);
        assert_eq!(observed.rule_kind, ScopeRuleKind::ExplicitInclude);
    }

    #[test]
    fn explicit_exclude_wins_before_include() {
        let scope = IndexScope::new(IndexScopeOptions {
            include_patterns: vec!["src/app.ts".to_string()],
            exclude_patterns: vec!["src/app.ts".to_string()],
            ..IndexScopeOptions::default()
        });
        let observed = scope.evaluate_path("src/app.ts", ScopePathKind::File, false);
        assert_eq!(observed.action, ScopeAction::WouldExclude);
        assert_eq!(observed.rule_kind, ScopeRuleKind::ExplicitExclude);
    }

    #[test]
    fn windows_backslash_paths_normalize_before_matching() {
        let observed = decision(r"target\debug\app.rs");
        assert_eq!(observed.normalized_path, "target/debug/app.rs");
        assert_eq!(observed.action, ScopeAction::WouldExclude);
        assert_eq!(observed.matched_rule.as_deref(), Some("hard_dir:target"));
    }

    #[test]
    fn component_matching_is_case_sensitive_like_linux_paths() {
        let observed = decision("Target/debug/app.rs");
        assert_eq!(observed.action, ScopeAction::WouldInclude);
        assert_eq!(observed.rule_kind, ScopeRuleKind::None);
    }

    #[test]
    fn excluded_source_paths_emit_warnings_instead_of_silence() {
        let observed = decision("reports/diagnostic_lab/fixture/tests/example.ts");
        assert_eq!(observed.action, ScopeAction::WouldExclude);
        assert!(observed
            .warnings
            .contains(&ScopeWarning::LikelySourceFileInsideWouldExcludedDir));
        assert!(observed
            .warnings
            .contains(&ScopeWarning::TestsFixturesExamplesDocsAffected));
    }

    #[test]
    fn gitignore_matcher_respects_negation_and_wildcards() {
        let matcher = GitIgnoreMatcher {
            patterns: vec![
                GitIgnorePattern::parse("/dist/*").expect("dist pattern"),
                GitIgnorePattern::parse("!/dist/keep.ts").expect("negation"),
                GitIgnorePattern::parse("*.log").expect("log pattern"),
            ],
        };
        assert!(matcher.is_ignored("dist/app.ts", ScopePathKind::File));
        assert!(!matcher.is_ignored("dist/keep.ts", ScopePathKind::File));
        assert!(matcher.is_ignored("nested/run.log", ScopePathKind::File));
    }
}
