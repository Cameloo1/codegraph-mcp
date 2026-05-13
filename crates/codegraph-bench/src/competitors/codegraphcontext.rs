//! Black-box external benchmark adapter for CodeGraphContext / CGC.
//!
//! This module is intentionally separate from the internal CodeGraph baseline
//! modes. It shells out to a discovered `cgc` or `codegraphcontext` executable,
//! captures raw artifacts, and normalizes only conservative file/symbol/path
//! evidence for fair comparison.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use codegraph_core::RelationKind;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    index_synthetic_repo, mean_reciprocal_rank, ndcg_at_k, precision_recall_f1, recall_at_k,
    run_baseline, BaselineMode, BenchResult, BenchmarkError, BenchmarkFamily, BenchmarkRepoSpec,
    BenchmarkTask, GroundTruth, SyntheticRepo,
};

pub const COMPETITOR_NAME: &str = "CodeGraphContext";
pub const COMPETITOR_TOOL_NAME: &str = "CGC";
pub const COMPETITOR_REPO_URL: &str = "https://github.com/CodeGraphContext/CodeGraphContext";
pub const OBSERVED_VERSION: &str = "0.4.7";
pub const COMPETITOR_BIN_ENV: &str = "CGC_COMPETITOR_BIN";
pub const COMPETITOR_BACKEND_ENV: &str = "CGC_DATABASE_BACKEND";
pub const COMPETITOR_HOME_ENV: &str = "CGC_COMPETITOR_HOME";
pub const DEFAULT_CGC_DATABASE_BACKEND: &str = "kuzudb";
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
pub const DEFAULT_TOP_K: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPlatformMetadata {
    pub os: String,
    pub arch: String,
    pub family: String,
}

impl HostPlatformMetadata {
    pub fn current() -> Self {
        Self {
            os: env::consts::OS.to_string(),
            arch: env::consts::ARCH.to_string(),
            family: env::consts::FAMILY.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompetitorManifest {
    pub competitor_name: String,
    pub source_repo_url: String,
    pub pinned_git_commit_sha: String,
    pub detected_package_version: String,
    pub observed_version_hint: String,
    pub python_version: String,
    pub executable_used: String,
    pub detected_database_backend: String,
    pub install_mode: String,
    pub benchmark_run_timestamp_unix_ms: u128,
    pub host_platform: HostPlatformMetadata,
}

impl CompetitorManifest {
    pub fn validate(&self) -> BenchResult<()> {
        let required = [
            ("competitor_name", self.competitor_name.as_str()),
            ("source_repo_url", self.source_repo_url.as_str()),
            ("pinned_git_commit_sha", self.pinned_git_commit_sha.as_str()),
            (
                "detected_package_version",
                self.detected_package_version.as_str(),
            ),
            ("python_version", self.python_version.as_str()),
            ("executable_used", self.executable_used.as_str()),
            (
                "detected_database_backend",
                self.detected_database_backend.as_str(),
            ),
            ("install_mode", self.install_mode.as_str()),
        ];
        for (field, value) in required {
            if value.trim().is_empty() {
                return Err(BenchmarkError::Validation(format!(
                    "competitor manifest field {field} must not be empty"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompetitorSetupPlan {
    pub source_repo_url: String,
    pub steps: Vec<String>,
    pub environment_variables: BTreeMap<String, String>,
    pub normal_tests_require_competitor: bool,
    pub network_required_during_benchmark: bool,
}

pub fn codegraphcontext_setup_plan() -> CompetitorSetupPlan {
    CompetitorSetupPlan {
        source_repo_url: COMPETITOR_REPO_URL.to_string(),
        steps: vec![
            format!("git clone {COMPETITOR_REPO_URL} <ignored-cache-dir>/CodeGraphContext"),
            "git -C <ignored-cache-dir>/CodeGraphContext rev-parse HEAD > pinned-commit.txt"
                .to_string(),
            "python -m venv <ignored-cache-dir>/CodeGraphContext/.venv".to_string(),
            "<venv>/python -m pip install --upgrade pip".to_string(),
            "<venv>/python -m pip install -e <ignored-cache-dir>/CodeGraphContext".to_string(),
            "discover executable with CGC_COMPETITOR_BIN, cgc, then codegraphcontext".to_string(),
        ],
        environment_variables: BTreeMap::from([
            (
                COMPETITOR_BIN_ENV.to_string(),
                "absolute path to cgc or codegraphcontext executable".to_string(),
            ),
            (
                "CGC_COMPETITOR_COMMIT".to_string(),
                "pinned git commit SHA recorded from the cloned competitor repo".to_string(),
            ),
            (
                "CGC_COMPETITOR_INSTALL_MODE".to_string(),
                "editable, local-wheel, or other local install mode".to_string(),
            ),
            (
                COMPETITOR_HOME_ENV.to_string(),
                "optional isolated home/cache root for benchmark subprocesses".to_string(),
            ),
            (
                COMPETITOR_BACKEND_ENV.to_string(),
                format!(
                    "detected or declared backend when available; defaults to {DEFAULT_CGC_DATABASE_BACKEND}"
                ),
            ),
            (
                "TREE_SITTER_LANGUAGE_PACK_CACHE_DIR".to_string(),
                "optional pre-populated tree-sitter-language-pack cache for offline parser loading"
                    .to_string(),
            ),
        ]),
        normal_tests_require_competitor: false,
        network_required_during_benchmark: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalComparisonMode {
    CodeGraphGraphOnly,
    CodeGraphFullContextPacket,
    CodeGraphContextCli,
}

impl ExternalComparisonMode {
    pub const ALL: [Self; 3] = [
        Self::CodeGraphGraphOnly,
        Self::CodeGraphFullContextPacket,
        Self::CodeGraphContextCli,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CodeGraphGraphOnly => "codegraph_graph_only",
            Self::CodeGraphFullContextPacket => "codegraph_full_context_packet",
            Self::CodeGraphContextCli => "codegraphcontext_cli",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalGroundTruth {
    pub task_id: String,
    pub query_text: String,
    pub expected_files: Vec<String>,
    pub expected_symbols: Vec<String>,
    pub expected_relation_sequence: Vec<String>,
    pub expected_path_symbols: Vec<String>,
    pub critical_source_spans: Vec<String>,
    pub unsupported_fields_allowed: Vec<String>,
}

impl ExternalGroundTruth {
    pub fn validate(&self) -> BenchResult<()> {
        if self.task_id.trim().is_empty() {
            return Err(BenchmarkError::Validation(
                "external ground truth task_id must not be empty".to_string(),
            ));
        }
        if self.query_text.trim().is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "external ground truth {} query_text must not be empty",
                self.task_id
            )));
        }
        if self.expected_files.is_empty()
            && self.expected_symbols.is_empty()
            && self.expected_path_symbols.is_empty()
        {
            return Err(BenchmarkError::Validation(format!(
                "external ground truth {} must include expected files, symbols, or path symbols",
                self.task_id
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalFixtureRepo {
    pub id: String,
    pub files: BTreeMap<String, String>,
    pub ground_truth: ExternalGroundTruth,
}

impl ExternalFixtureRepo {
    pub fn validate(&self) -> BenchResult<()> {
        if self.id.trim().is_empty() {
            return Err(BenchmarkError::Validation(
                "external fixture id must not be empty".to_string(),
            ));
        }
        if self.files.is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "external fixture {} must include files",
                self.id
            )));
        }
        self.ground_truth.validate()
    }

    pub fn write_to(&self, root: &Path) -> BenchResult<()> {
        for (relative_path, source) in &self.files {
            let path = root.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, source)?;
        }
        fs::write(
            root.join("ground_truth.json"),
            serde_json::to_string_pretty(&self.ground_truth)
                .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeGraphContextCommandCapture {
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub latency_ms: u64,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationMode {
    Json,
    HumanText,
    UnsupportedOrUnparseable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedCompetitorOutput {
    pub files: Vec<String>,
    pub symbols: Vec<String>,
    pub path_symbols: Vec<String>,
    pub relation_sequence: Vec<String>,
    pub source_spans: Vec<String>,
    pub unsupported_fields: Vec<String>,
    pub mode: NormalizationMode,
    pub warnings: Vec<String>,
    pub raw_json: Option<Value>,
}

impl NormalizedCompetitorOutput {
    fn empty_unsupported(reason: impl Into<String>) -> Self {
        Self {
            files: Vec::new(),
            symbols: Vec::new(),
            path_symbols: Vec::new(),
            relation_sequence: Vec::new(),
            source_spans: Vec::new(),
            unsupported_fields: vec![reason.into()],
            mode: NormalizationMode::UnsupportedOrUnparseable,
            warnings: Vec::new(),
            raw_json: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalComparisonMetrics {
    pub file_recall_at_k: BTreeMap<String, f64>,
    pub symbol_recall_at_k: BTreeMap<String, f64>,
    pub path_recall_at_k: BTreeMap<String, f64>,
    pub relation_precision: f64,
    pub relation_recall: f64,
    pub relation_f1: f64,
    pub mrr: f64,
    pub ndcg: f64,
    pub source_span_coverage: f64,
    pub false_proof_count: u64,
    pub unsupported_feature_count: u64,
    pub index_latency_ms: u64,
    pub query_latency_ms: u64,
    pub estimated_token_cost: u64,
    pub memory_usage_bytes: Option<u64>,
    pub memory_usage_note: String,
}

impl ExternalComparisonMetrics {
    fn empty() -> Self {
        Self {
            file_recall_at_k: BTreeMap::new(),
            symbol_recall_at_k: BTreeMap::new(),
            path_recall_at_k: BTreeMap::new(),
            relation_precision: 0.0,
            relation_recall: 0.0,
            relation_f1: 0.0,
            mrr: 0.0,
            ndcg: 0.0,
            source_span_coverage: 0.0,
            false_proof_count: 0,
            unsupported_feature_count: 0,
            index_latency_ms: 0,
            query_latency_ms: 0,
            estimated_token_cost: 0,
            memory_usage_bytes: None,
            memory_usage_note: "unknown".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalComparisonStatus {
    Completed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalComparisonRun {
    pub fixture_id: String,
    pub task_id: String,
    pub mode: ExternalComparisonMode,
    pub status: ExternalComparisonStatus,
    pub skip_reason: Option<String>,
    pub metrics: ExternalComparisonMetrics,
    pub normalized_output: NormalizedCompetitorOutput,
    pub raw_artifact_paths: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalComparisonReport {
    pub schema_version: u32,
    pub benchmark_id: String,
    pub generated_by: String,
    pub manifest: CompetitorManifest,
    pub setup_plan: CompetitorSetupPlan,
    pub fairness_rules: Vec<String>,
    pub report_dir: String,
    pub runs: Vec<ExternalComparisonRun>,
    pub aggregate: BTreeMap<String, ExternalComparisonAggregate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalComparisonAggregate {
    pub runs: usize,
    pub skipped: usize,
    pub average_file_recall_at_10: f64,
    pub average_symbol_recall_at_10: f64,
    pub average_path_recall_at_10: f64,
    pub average_relation_f1: f64,
    pub average_mrr: f64,
    pub average_ndcg: f64,
    pub unsupported_feature_count: u64,
    pub false_proof_count: u64,
    pub total_index_latency_ms: u64,
    pub total_query_latency_ms: u64,
}

#[derive(Debug, Clone)]
pub struct CodeGraphContextComparisonOptions {
    pub report_dir: PathBuf,
    pub timeout_ms: u64,
    pub top_k: usize,
    pub competitor_executable: Option<PathBuf>,
}

impl CodeGraphContextComparisonOptions {
    pub fn with_report_dir(report_dir: PathBuf) -> Self {
        Self {
            report_dir,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            top_k: DEFAULT_TOP_K,
            competitor_executable: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CodeGraphContextRunner {
    executable: PathBuf,
    timeout_ms: u64,
    home_override: Option<PathBuf>,
}

impl CodeGraphContextRunner {
    pub fn discover(timeout_ms: u64) -> Result<Self, String> {
        if let Some(raw) = env::var_os(COMPETITOR_BIN_ENV) {
            let path = PathBuf::from(raw);
            if path.exists() {
                return Ok(Self {
                    executable: path,
                    timeout_ms,
                    home_override: env::var_os(COMPETITOR_HOME_ENV).map(PathBuf::from),
                });
            }
            return Err(format!(
                "{COMPETITOR_BIN_ENV} was set but executable does not exist: {}",
                path.display()
            ));
        }

        for name in ["cgc", "codegraphcontext"] {
            if let Some(path) = find_executable_on_path(name) {
                return Ok(Self {
                    executable: path,
                    timeout_ms,
                    home_override: env::var_os(COMPETITOR_HOME_ENV).map(PathBuf::from),
                });
            }
        }

        if let Some(path) = repo_local_cgc_executable() {
            return Ok(Self {
                executable: path,
                timeout_ms,
                home_override: Some(default_repo_local_cgc_home()),
            });
        }

        Err(format!(
            "CodeGraphContext executable unavailable; set {COMPETITOR_BIN_ENV} or install cgc/codegraphcontext"
        ))
    }

    pub fn with_executable(executable: PathBuf, timeout_ms: u64) -> Self {
        Self {
            executable,
            timeout_ms,
            home_override: env::var_os(COMPETITOR_HOME_ENV).map(PathBuf::from),
        }
    }

    pub fn with_executable_and_home(
        executable: PathBuf,
        timeout_ms: u64,
        home_override: Option<PathBuf>,
    ) -> Self {
        Self {
            executable,
            timeout_ms,
            home_override,
        }
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn manifest(&self) -> CompetitorManifest {
        let version_capture = self.run_raw(&["--version"], Path::new("."));
        let detected_package_version = version_capture
            .ok()
            .and_then(|capture| {
                parse_version_from_text(&format!("{}\n{}", capture.stdout, capture.stderr))
            })
            .unwrap_or_else(|| "unknown".to_string());
        CompetitorManifest {
            competitor_name: COMPETITOR_NAME.to_string(),
            source_repo_url: COMPETITOR_REPO_URL.to_string(),
            pinned_git_commit_sha: env::var("CGC_COMPETITOR_COMMIT")
                .unwrap_or_else(|_| "unknown".to_string()),
            detected_package_version,
            observed_version_hint: OBSERVED_VERSION.to_string(),
            python_version: detect_python_version_for_executable(&self.executable),
            executable_used: self.executable.display().to_string(),
            detected_database_backend: competitor_backend(),
            install_mode: env::var("CGC_COMPETITOR_INSTALL_MODE")
                .unwrap_or_else(|_| "unknown".to_string()),
            benchmark_run_timestamp_unix_ms: unix_ms(),
            host_platform: HostPlatformMetadata::current(),
        }
    }

    pub fn index_repo(&self, repo_root: &Path) -> CodeGraphContextCommandCapture {
        let repo_arg = absolute_path(repo_root).display().to_string();
        let attempts = vec![
            self.with_backend(&["index", "--force", &repo_arg]),
            self.with_backend(&["index", &repo_arg, "--force"]),
            self.with_backend(&["index", &repo_arg]),
            self.with_backend(&["index", "--path", &repo_arg]),
        ];
        self.run_first_success(&attempts, repo_root)
    }

    pub fn symbol_search(
        &self,
        repo_root: &Path,
        query: &str,
        top_k: usize,
    ) -> CodeGraphContextCommandCapture {
        let top_k_arg = top_k.to_string();
        let attempts = vec![
            self.with_backend(&["find", "name", query]),
            self.with_backend(&["find", "pattern", query]),
            self.with_backend(&["find", "content", query]),
            self.with_backend(&["search", query, "--top-k", &top_k_arg]),
            self.with_backend(&["symbols", query]),
            self.with_backend(&["query", "symbols", query, "--top-k", &top_k_arg]),
        ];
        self.run_first_success(&attempts, repo_root)
    }

    pub fn callers(&self, repo_root: &Path, symbol: &str) -> CodeGraphContextCommandCapture {
        let attempts = vec![
            self.with_backend(&["analyze", "callers", symbol]),
            self.with_backend(&["callers", symbol]),
            self.with_backend(&["caller", symbol]),
            self.with_backend(&["query", "callers", symbol]),
        ];
        self.run_first_success(&attempts, repo_root)
    }

    pub fn callees(&self, repo_root: &Path, symbol: &str) -> CodeGraphContextCommandCapture {
        let attempts = vec![
            self.with_backend(&["analyze", "calls", symbol]),
            self.with_backend(&["callees", symbol]),
            self.with_backend(&["callee", symbol]),
            self.with_backend(&["query", "callees", symbol]),
        ];
        self.run_first_success(&attempts, repo_root)
    }

    pub fn call_chain(
        &self,
        repo_root: &Path,
        source: &str,
        target: &str,
    ) -> CodeGraphContextCommandCapture {
        let attempts = vec![
            self.with_backend(&["analyze", "chain", source, target, "--depth", "10"]),
            self.with_backend(&["call-chain", source, target]),
            self.with_backend(&["chain", source, target]),
            self.with_backend(&["query", "path", source, target]),
        ];
        self.run_first_success(&attempts, repo_root)
    }

    fn with_backend(&self, args: &[&str]) -> Vec<String> {
        let mut command = vec!["--database".to_string(), competitor_backend()];
        command.extend(args.iter().map(|arg| (*arg).to_string()));
        command
    }

    fn run_first_success(
        &self,
        attempts: &[Vec<String>],
        cwd: &Path,
    ) -> CodeGraphContextCommandCapture {
        let mut captures = Vec::new();
        for args in attempts {
            let capture = self.run_raw(&args.iter().map(String::as_str).collect::<Vec<_>>(), cwd);
            match capture {
                Ok(capture) if is_successful_cgc_capture(&capture) => return capture,
                Ok(capture) => captures.push(capture),
                Err(message) => {
                    return CodeGraphContextCommandCapture {
                        executable: self.executable.display().to_string(),
                        args: args.clone(),
                        cwd: cwd.display().to_string(),
                        stdout: String::new(),
                        stderr: message,
                        exit_code: None,
                        latency_ms: 0,
                        timed_out: false,
                    }
                }
            }
        }
        captures
            .into_iter()
            .last()
            .unwrap_or_else(|| CodeGraphContextCommandCapture {
                executable: self.executable.display().to_string(),
                args: Vec::new(),
                cwd: cwd.display().to_string(),
                stdout: String::new(),
                stderr: "no command attempts configured".to_string(),
                exit_code: None,
                latency_ms: 0,
                timed_out: false,
            })
    }

    fn run_raw(&self, args: &[&str], cwd: &Path) -> Result<CodeGraphContextCommandCapture, String> {
        let start = Instant::now();
        let mut command = Command::new(&self.executable);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let home_override = env::var_os(COMPETITOR_HOME_ENV)
            .map(PathBuf::from)
            .or_else(|| self.home_override.clone());
        if let Some(home_path) = home_override {
            let local_app_data = home_path.join("AppData").join("Local");
            let roaming_app_data = home_path.join("AppData").join("Roaming");
            let _ = fs::create_dir_all(&local_app_data);
            let _ = fs::create_dir_all(&roaming_app_data);
            command
                .env("HOME", &home_path)
                .env("USERPROFILE", &home_path)
                .env("LOCALAPPDATA", local_app_data)
                .env("APPDATA", roaming_app_data);
        }
        command
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .env("DEFAULT_DATABASE", competitor_backend())
            .env("CGC_RUNTIME_DB_TYPE", competitor_backend())
            .env("INDEX_SOURCE", "true")
            .env("IGNORE_HIDDEN_FILES", "false");
        if env::var_os("PYTHONUTF8").is_none() {
            command.env("PYTHONUTF8", "1");
        }
        if env::var_os("PYTHONIOENCODING").is_none() {
            command.env("PYTHONIOENCODING", "utf-8");
        }
        let mut child = command.spawn().map_err(|error| error.to_string())?;

        let timeout = Duration::from_millis(self.timeout_ms);
        let mut timed_out = false;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if start.elapsed() >= timeout => {
                    timed_out = true;
                    let _ = child.kill();
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => return Err(error.to_string()),
            }
        }

        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        Ok(CodeGraphContextCommandCapture {
            executable: self.executable.display().to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            cwd: cwd.display().to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code(),
            latency_ms: start.elapsed().as_millis() as u64,
            timed_out,
        })
    }
}

fn default_repo_local_cgc_home() -> PathBuf {
    PathBuf::from(".tools")
        .join("cgc_recovery")
        .join("home_shared")
}

fn repo_local_cgc_executable() -> Option<PathBuf> {
    repo_local_cgc_candidate_paths()
        .into_iter()
        .map(|path| absolute_path(&path))
        .find(|path| path.exists())
}

fn repo_local_cgc_candidate_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from(".tools")
            .join("cgc_recovery")
            .join("venv312_compat")
            .join("Scripts")
            .join("cgc.exe"),
        PathBuf::from("target")
            .join("cgc-competitor")
            .join(".venv312-pypi")
            .join("Scripts")
            .join("cgc.exe"),
        PathBuf::from("target")
            .join("cgc-competitor")
            .join(".venv312-pypi")
            .join("Scripts")
            .join("codegraphcontext.exe"),
    ]
}

fn is_successful_cgc_capture(capture: &CodeGraphContextCommandCapture) -> bool {
    if capture.exit_code != Some(0) || capture.timed_out {
        return false;
    }
    let text = format!("{}\n{}", capture.stdout, capture.stderr).to_ascii_lowercase();
    !(text.contains("traceback")
        || text.contains("database configuration error")
        || text.contains("error: path does not exist")
        || text.contains("an error occurred while executing query"))
}

pub fn run_codegraphcontext_comparison(
    options: CodeGraphContextComparisonOptions,
) -> BenchResult<ExternalComparisonReport> {
    fs::create_dir_all(&options.report_dir)?;
    let fixtures = external_competitor_fixtures();
    for fixture in &fixtures {
        fixture.validate()?;
    }

    let runner = match options.competitor_executable.clone() {
        Some(path) if path.exists() => Some(Ok(CodeGraphContextRunner::with_executable(
            path,
            options.timeout_ms,
        ))),
        Some(path) => Some(Err(format!(
            "configured competitor executable does not exist: {}",
            path.display()
        ))),
        None => Some(CodeGraphContextRunner::discover(options.timeout_ms)),
    };

    let manifest = match &runner {
        Some(Ok(runner)) => runner.manifest(),
        Some(Err(reason)) => skipped_manifest(reason),
        None => skipped_manifest("competitor discovery was not attempted"),
    };
    manifest.validate()?;

    let mut runs = Vec::new();
    for fixture in &fixtures {
        let fixture_root = options.report_dir.join("fixtures").join(&fixture.id);
        fixture.write_to(&fixture_root)?;

        runs.push(run_codegraph_mode(
            fixture,
            ExternalComparisonMode::CodeGraphGraphOnly,
            options.top_k,
            &options.report_dir,
        )?);
        runs.push(run_codegraph_mode(
            fixture,
            ExternalComparisonMode::CodeGraphFullContextPacket,
            options.top_k,
            &options.report_dir,
        )?);

        match &runner {
            Some(Ok(runner)) => runs.push(run_codegraphcontext_mode(
                fixture,
                &fixture_root,
                runner,
                options.top_k,
                &options.report_dir,
            )?),
            Some(Err(reason)) => runs.push(skipped_external_run(fixture, reason)),
            None => runs.push(skipped_external_run(
                fixture,
                "competitor discovery was not attempted",
            )),
        }
    }

    let report = ExternalComparisonReport {
        schema_version: 1,
        benchmark_id: "codegraphcontext-external-comparison".to_string(),
        generated_by: "codegraph-bench phase 21.1".to_string(),
        manifest,
        setup_plan: codegraphcontext_setup_plan(),
        fairness_rules: fairness_rules(),
        report_dir: options.report_dir.display().to_string(),
        aggregate: aggregate_external_runs(&runs),
        runs,
    };
    write_external_report(&report, &options.report_dir)?;
    Ok(report)
}

pub fn default_report_dir() -> PathBuf {
    PathBuf::from("reports")
        .join("cgc-comparison")
        .join(format!("{}", unix_ms()))
}

pub fn external_competitor_fixtures() -> Vec<ExternalFixtureRepo> {
    vec![
        ts_call_chain_fixture(),
        auth_route_fixture(),
        event_flow_fixture(),
        mutation_impact_fixture(),
        test_impact_fixture(),
    ]
}

pub fn normalize_codegraphcontext_text(
    stdout: &str,
    stderr: &str,
    repo_root: Option<&Path>,
) -> NormalizedCompetitorOutput {
    if stdout.trim().is_empty() && stderr.trim().is_empty() {
        return NormalizedCompetitorOutput::empty_unsupported("empty stdout/stderr");
    }

    if let Ok(value) = serde_json::from_str::<Value>(stdout) {
        let mut output = NormalizedCompetitorOutput {
            files: Vec::new(),
            symbols: Vec::new(),
            path_symbols: Vec::new(),
            relation_sequence: Vec::new(),
            source_spans: Vec::new(),
            unsupported_fields: Vec::new(),
            mode: NormalizationMode::Json,
            warnings: Vec::new(),
            raw_json: Some(value.clone()),
        };
        collect_json_evidence(&value, repo_root, &mut output);
        normalize_lists(&mut output);
        if output.files.is_empty()
            && output.symbols.is_empty()
            && output.path_symbols.is_empty()
            && output.relation_sequence.is_empty()
        {
            output.mode = NormalizationMode::UnsupportedOrUnparseable;
            output
                .unsupported_fields
                .push("json contained no conservative file/symbol/path evidence".to_string());
        }
        return output;
    }

    let mut output = NormalizedCompetitorOutput {
        files: Vec::new(),
        symbols: Vec::new(),
        path_symbols: Vec::new(),
        relation_sequence: Vec::new(),
        source_spans: Vec::new(),
        unsupported_fields: Vec::new(),
        mode: NormalizationMode::HumanText,
        warnings: Vec::new(),
        raw_json: None,
    };

    for line in stdout.lines().chain(stderr.lines()) {
        collect_text_line_evidence(line, repo_root, &mut output);
    }
    normalize_lists(&mut output);
    if output.files.is_empty()
        && output.symbols.is_empty()
        && output.path_symbols.is_empty()
        && output.relation_sequence.is_empty()
    {
        output.mode = NormalizationMode::UnsupportedOrUnparseable;
        output
            .unsupported_fields
            .push("human-readable output was unsupported_or_unparseable".to_string());
    }
    output
}

pub fn calculate_external_metrics(
    truth: &ExternalGroundTruth,
    normalized: &NormalizedCompetitorOutput,
    top_k: usize,
    index_latency_ms: u64,
    query_latency_ms: u64,
) -> ExternalComparisonMetrics {
    let mut metrics = ExternalComparisonMetrics::empty();
    let k_values = [1usize, 3, 5, top_k.max(1)];
    let expected_files = set_from(&truth.expected_files);
    let expected_symbols = set_from(&truth.expected_symbols);
    let expected_path_symbols = set_from(&truth.expected_path_symbols);
    for k in k_values {
        metrics.file_recall_at_k.insert(
            k.to_string(),
            recall_at_k(&expected_files, &normalized.files, k),
        );
        metrics.symbol_recall_at_k.insert(
            k.to_string(),
            recall_at_k(&expected_symbols, &normalized.symbols, k),
        );
        metrics.path_recall_at_k.insert(
            k.to_string(),
            recall_at_k(&expected_path_symbols, &normalized.path_symbols, k),
        );
    }

    let relation_score = precision_recall_f1(
        &set_from(&normalize_relation_names(&truth.expected_relation_sequence)),
        &set_from(&normalize_relation_names(&normalized.relation_sequence)),
    );
    metrics.relation_precision = relation_score.precision;
    metrics.relation_recall = relation_score.recall;
    metrics.relation_f1 = relation_score.f1;

    let mut ranked = Vec::new();
    ranked.extend(normalized.files.iter().cloned());
    ranked.extend(normalized.symbols.iter().cloned());
    ranked.extend(normalized.path_symbols.iter().cloned());
    let mut expected_ranked = truth.expected_files.clone();
    expected_ranked.extend(truth.expected_symbols.iter().cloned());
    expected_ranked.extend(truth.expected_path_symbols.iter().cloned());
    let expected_ranked = set_from(&expected_ranked);
    metrics.mrr = mean_reciprocal_rank(&expected_ranked, &ranked);
    metrics.ndcg = ndcg_at_k(&expected_ranked, &ranked, top_k.max(1));

    metrics.source_span_coverage = recall_at_k(
        &set_from(&truth.critical_source_spans),
        &normalized.source_spans,
        usize::MAX,
    );
    metrics.false_proof_count = false_proof_count(truth, normalized);
    metrics.unsupported_feature_count = normalized.unsupported_fields.len() as u64;
    metrics.index_latency_ms = index_latency_ms;
    metrics.query_latency_ms = query_latency_ms;
    metrics.estimated_token_cost = estimate_token_cost(&truth.query_text, normalized);
    metrics
}

fn run_codegraph_mode(
    fixture: &ExternalFixtureRepo,
    mode: ExternalComparisonMode,
    top_k: usize,
    report_dir: &Path,
) -> BenchResult<ExternalComparisonRun> {
    let task = benchmark_task_for_external_fixture(fixture);
    let repo = SyntheticRepo {
        id: fixture.id.clone(),
        kind: crate::SyntheticRepoKind::ContextRetrieval,
        files: fixture.files.clone(),
        tasks: vec![task.clone()],
    };
    let index_start = Instant::now();
    let corpus = index_synthetic_repo(&repo)?;
    let index_latency_ms = index_start.elapsed().as_millis() as u64;
    let baseline = match mode {
        ExternalComparisonMode::CodeGraphGraphOnly => BaselineMode::GraphOnly,
        ExternalComparisonMode::CodeGraphFullContextPacket => BaselineMode::FullContextPacket,
        ExternalComparisonMode::CodeGraphContextCli => {
            return Err(BenchmarkError::Validation(
                "CodeGraphContext mode is not a CodeGraph internal baseline".to_string(),
            ))
        }
    };
    let query_start = Instant::now();
    let result = run_baseline(&task, &corpus, baseline)?;
    let query_latency_ms = query_start.elapsed().as_millis() as u64;
    let normalized = normalize_codegraph_result(&result);
    let metrics = calculate_external_metrics(
        &fixture.ground_truth,
        &normalized,
        top_k,
        index_latency_ms,
        query_latency_ms,
    );
    let normalized_path = report_dir
        .join("normalized_outputs")
        .join("codegraph")
        .join(format!("{}_{}.json", fixture.id, mode.as_str()));
    write_json_file(&normalized_path, &normalized)?;

    Ok(ExternalComparisonRun {
        fixture_id: fixture.id.clone(),
        task_id: fixture.ground_truth.task_id.clone(),
        mode,
        status: ExternalComparisonStatus::Completed,
        skip_reason: None,
        metrics,
        normalized_output: normalized,
        raw_artifact_paths: vec![normalized_path.display().to_string()],
        warnings: Vec::new(),
    })
}

fn run_codegraphcontext_mode(
    fixture: &ExternalFixtureRepo,
    fixture_root: &Path,
    runner: &CodeGraphContextRunner,
    top_k: usize,
    report_dir: &Path,
) -> BenchResult<ExternalComparisonRun> {
    let index_capture = runner.index_repo(fixture_root);
    let query_symbol = fixture
        .ground_truth
        .expected_symbols
        .first()
        .cloned()
        .unwrap_or_else(|| fixture.ground_truth.query_text.clone());
    let source = fixture
        .ground_truth
        .expected_path_symbols
        .first()
        .cloned()
        .unwrap_or_else(|| query_symbol.clone());
    let target = fixture
        .ground_truth
        .expected_path_symbols
        .last()
        .cloned()
        .unwrap_or_else(|| query_symbol.clone());

    let captures = [
        index_capture,
        runner.symbol_search(fixture_root, &query_symbol, top_k),
        runner.callers(fixture_root, &query_symbol),
        runner.callees(fixture_root, &query_symbol),
        runner.call_chain(fixture_root, &source, &target),
    ];
    let index_latency_ms = captures
        .first()
        .map(|capture| capture.latency_ms)
        .unwrap_or(0);
    let query_latency_ms = captures
        .iter()
        .skip(1)
        .map(|capture| capture.latency_ms)
        .sum::<u64>();

    let combined_stdout = captures
        .iter()
        .map(|capture| capture.stdout.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let combined_stderr = captures
        .iter()
        .map(|capture| capture.stderr.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let normalized =
        normalize_codegraphcontext_text(&combined_stdout, &combined_stderr, Some(fixture_root));
    let metrics = calculate_external_metrics(
        &fixture.ground_truth,
        &normalized,
        top_k,
        index_latency_ms,
        query_latency_ms,
    );

    let mut artifact_paths = Vec::new();
    let raw_dir = report_dir
        .join("raw_artifacts")
        .join("codegraphcontext")
        .join(&fixture.id);
    fs::create_dir_all(&raw_dir)?;
    for (index, capture) in captures.iter().enumerate() {
        let path = raw_dir.join(format!("{index}.json"));
        write_json_file(&path, capture)?;
        artifact_paths.push(path.display().to_string());
    }
    let normalized_path = report_dir
        .join("normalized_outputs")
        .join("codegraphcontext")
        .join(format!("{}.json", fixture.id));
    write_json_file(&normalized_path, &normalized)?;
    artifact_paths.push(normalized_path.display().to_string());

    Ok(ExternalComparisonRun {
        fixture_id: fixture.id.clone(),
        task_id: fixture.ground_truth.task_id.clone(),
        mode: ExternalComparisonMode::CodeGraphContextCli,
        status: ExternalComparisonStatus::Completed,
        skip_reason: None,
        metrics,
        normalized_output: normalized,
        raw_artifact_paths: artifact_paths,
        warnings: captures
            .iter()
            .filter(|capture| capture.exit_code != Some(0) || capture.timed_out)
            .map(|capture| {
                format!(
                    "command {:?} exited {:?}, timed_out={}",
                    capture.args, capture.exit_code, capture.timed_out
                )
            })
            .collect(),
    })
}

fn skipped_external_run(fixture: &ExternalFixtureRepo, reason: &str) -> ExternalComparisonRun {
    let normalized = NormalizedCompetitorOutput::empty_unsupported(reason);
    ExternalComparisonRun {
        fixture_id: fixture.id.clone(),
        task_id: fixture.ground_truth.task_id.clone(),
        mode: ExternalComparisonMode::CodeGraphContextCli,
        status: ExternalComparisonStatus::Skipped,
        skip_reason: Some(reason.to_string()),
        metrics: calculate_external_metrics(
            &fixture.ground_truth,
            &normalized,
            DEFAULT_TOP_K,
            0,
            0,
        ),
        normalized_output: normalized,
        raw_artifact_paths: Vec::new(),
        warnings: vec![reason.to_string()],
    }
}

fn normalize_codegraph_result(result: &crate::BenchmarkRunResult) -> NormalizedCompetitorOutput {
    let mut output = NormalizedCompetitorOutput {
        files: result.retrieved_files.clone(),
        symbols: result
            .retrieved_symbols
            .iter()
            .map(|symbol| compact_entity_id(symbol))
            .collect(),
        path_symbols: result
            .retrieved_paths
            .iter()
            .flat_map(|path| {
                [
                    compact_entity_id(&path.source),
                    compact_entity_id(&path.target),
                ]
            })
            .collect(),
        relation_sequence: result
            .retrieved_paths
            .iter()
            .flat_map(|path| path.relations.iter().map(|relation| relation.to_string()))
            .collect(),
        source_spans: result
            .retrieved_paths
            .iter()
            .flat_map(|path| path.source_spans.iter().cloned())
            .collect(),
        unsupported_fields: Vec::new(),
        mode: NormalizationMode::Json,
        warnings: result.warnings.clone(),
        raw_json: result.context_packet.clone(),
    };
    for relation in &result.observed_relations {
        output.files.push(relation.repo_relative_path.clone());
        output.symbols.push(compact_entity_id(&relation.head_id));
        output.symbols.push(compact_entity_id(&relation.tail_id));
    }
    if let Some(packet) = &result.context_packet {
        collect_json_evidence(packet, None, &mut output);
    }
    normalize_lists(&mut output);
    output
}

fn benchmark_task_for_external_fixture(fixture: &ExternalFixtureRepo) -> BenchmarkTask {
    let relation_sequence = fixture
        .ground_truth
        .expected_relation_sequence
        .iter()
        .filter_map(|relation| relation.parse::<RelationKind>().ok())
        .collect::<Vec<_>>();
    BenchmarkTask {
        id: fixture.ground_truth.task_id.clone(),
        family: family_for_fixture(&fixture.id),
        prompt: fixture.ground_truth.query_text.clone(),
        repo: BenchmarkRepoSpec::Synthetic {
            kind: crate::SyntheticRepoKind::ContextRetrieval,
        },
        ground_truth: GroundTruth {
            expected_relations: Vec::new(),
            expected_relation_sequences: if relation_sequence.is_empty() {
                Vec::new()
            } else {
                vec![relation_sequence]
            },
            expected_files: fixture.ground_truth.expected_files.clone(),
            expected_symbols: fixture.ground_truth.expected_symbols.clone(),
            expected_tests: if fixture.id == "test-impact" {
                fixture.ground_truth.expected_files.clone()
            } else {
                Vec::new()
            },
            expected_patch_success: None,
            expected_test_success: None,
            metadata: BTreeMap::from([
                ("external_competitor_fixture".to_string(), json!(fixture.id)),
                ("source_of_truth".to_string(), json!("MVP.md + prompt 21.1")),
            ]),
        },
        k_values: vec![1, 3, 5, DEFAULT_TOP_K],
        metadata: BTreeMap::new(),
    }
}

fn family_for_fixture(id: &str) -> BenchmarkFamily {
    match id {
        "auth-route" => BenchmarkFamily::SecurityAuth,
        "event-flow" => BenchmarkFamily::AsyncEvent,
        "mutation-impact" => BenchmarkFamily::RelationExtraction,
        "test-impact" => BenchmarkFamily::TestImpact,
        _ => BenchmarkFamily::LongChainPath,
    }
}

fn ts_call_chain_fixture() -> ExternalFixtureRepo {
    let mut files = BTreeMap::new();
    files.insert(
        "src/api.ts".to_string(),
        r#"
import { createInvoice } from "./service";

export function postInvoice(req: any) {
  return createInvoice(req.body);
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "src/service.ts".to_string(),
        r#"
import { saveInvoice } from "./repo";

export function createInvoice(payload: any) {
  const record = { id: payload.id, amount: payload.amount };
  return saveInvoice(record);
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "src/repo.ts".to_string(),
        r#"
import { writeDatabase } from "./db";

export function saveInvoice(record: any) {
  return writeDatabase("invoices", record);
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "src/db.ts".to_string(),
        r#"
export function writeDatabase(table: string, value: any) {
  return { table, value, written: true };
}
"#
        .trim()
        .to_string(),
    );
    ExternalFixtureRepo {
        id: "ts-call-chain".to_string(),
        files,
        ground_truth: ExternalGroundTruth {
            task_id: "ts-call-chain".to_string(),
            query_text: "Trace how postInvoice reaches the database write".to_string(),
            expected_files: vec![
                "src/api.ts".to_string(),
                "src/service.ts".to_string(),
                "src/repo.ts".to_string(),
                "src/db.ts".to_string(),
            ],
            expected_symbols: vec![
                "postInvoice".to_string(),
                "createInvoice".to_string(),
                "saveInvoice".to_string(),
                "writeDatabase".to_string(),
            ],
            expected_relation_sequence: vec![
                "CALLS".to_string(),
                "CALLS".to_string(),
                "CALLS".to_string(),
            ],
            expected_path_symbols: vec![
                "postInvoice".to_string(),
                "createInvoice".to_string(),
                "saveInvoice".to_string(),
                "writeDatabase".to_string(),
            ],
            critical_source_spans: vec![
                "src/api.ts:3-5".to_string(),
                "src/repo.ts:3-5".to_string(),
            ],
            unsupported_fields_allowed: vec!["exact_relation_taxonomy".to_string()],
        },
    }
}

fn auth_route_fixture() -> ExternalFixtureRepo {
    let mut files = BTreeMap::new();
    files.insert(
        "src/routes.ts".to_string(),
        r#"
import { authorizeRequest, checkRole } from "./security";

export function adminRoute(req: any) {
  authorizeRequest(req);
  checkRole(req.user, "admin");
  return { ok: true };
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "src/security.ts".to_string(),
        r#"
export function authorizeRequest(req: any) {
  if (!req.user) throw new Error("unauthorized");
}

export function checkRole(user: any, role: string) {
  if (user.role !== role) throw new Error("forbidden");
}
"#
        .trim()
        .to_string(),
    );
    ExternalFixtureRepo {
        id: "auth-route".to_string(),
        files,
        ground_truth: ExternalGroundTruth {
            task_id: "auth-route".to_string(),
            query_text: "Find the auth and role-check path for adminRoute".to_string(),
            expected_files: vec!["src/routes.ts".to_string(), "src/security.ts".to_string()],
            expected_symbols: vec![
                "adminRoute".to_string(),
                "authorizeRequest".to_string(),
                "checkRole".to_string(),
            ],
            expected_relation_sequence: vec![
                "EXPOSES".to_string(),
                "CALLS".to_string(),
                "AUTHORIZES".to_string(),
                "CHECKS_ROLE".to_string(),
            ],
            expected_path_symbols: vec![
                "adminRoute".to_string(),
                "authorizeRequest".to_string(),
                "checkRole".to_string(),
            ],
            critical_source_spans: vec!["src/routes.ts:3-7".to_string()],
            unsupported_fields_allowed: vec!["exact_relation_taxonomy".to_string()],
        },
    }
}

fn event_flow_fixture() -> ExternalFixtureRepo {
    let mut files = BTreeMap::new();
    files.insert(
        "src/events.ts".to_string(),
        r#"
type Handler = (payload: any) => void;
const listeners: Record<string, Handler[]> = {};

export function publishUserCreated(payload: any) {
  emitEvent("user.created", payload);
}

export function emitEvent(name: string, payload: any) {
  for (const handler of listeners[name] || []) handler(payload);
}

export function onEvent(name: string, handler: Handler) {
  listeners[name] = [...(listeners[name] || []), handler];
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "src/consumer.ts".to_string(),
        r#"
import { onEvent } from "./events";
import { markUserActive } from "./state";

export function registerUserConsumer() {
  onEvent("user.created", handleUserCreated);
}

export function handleUserCreated(payload: any) {
  markUserActive(payload.id);
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "src/state.ts".to_string(),
        r#"
const state: Record<string, boolean> = {};

export function markUserActive(id: string) {
  state[id] = true;
}
"#
        .trim()
        .to_string(),
    );
    ExternalFixtureRepo {
        id: "event-flow".to_string(),
        files,
        ground_truth: ExternalGroundTruth {
            task_id: "event-flow".to_string(),
            query_text: "Trace user.created event from publisher to state mutation".to_string(),
            expected_files: vec![
                "src/events.ts".to_string(),
                "src/consumer.ts".to_string(),
                "src/state.ts".to_string(),
            ],
            expected_symbols: vec![
                "publishUserCreated".to_string(),
                "emitEvent".to_string(),
                "onEvent".to_string(),
                "handleUserCreated".to_string(),
                "markUserActive".to_string(),
            ],
            expected_relation_sequence: vec![
                "PUBLISHES".to_string(),
                "EMITS".to_string(),
                "LISTENS_TO".to_string(),
                "CALLS".to_string(),
                "MUTATES".to_string(),
            ],
            expected_path_symbols: vec![
                "publishUserCreated".to_string(),
                "emitEvent".to_string(),
                "handleUserCreated".to_string(),
                "markUserActive".to_string(),
            ],
            critical_source_spans: vec![
                "src/events.ts:4-10".to_string(),
                "src/consumer.ts:8-10".to_string(),
            ],
            unsupported_fields_allowed: vec!["exact_relation_taxonomy".to_string()],
        },
    }
}

fn mutation_impact_fixture() -> ExternalFixtureRepo {
    let mut files = BTreeMap::new();
    files.insert(
        "src/cart.ts".to_string(),
        r#"
import { persistCart } from "./repo";

export const cartState: { total: number } = { total: 0 };

export function addToCart(price: number) {
  cartState.total = cartState.total + price;
  return persistCart(cartState);
}

export function checkout(price: number) {
  return addToCart(price);
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "src/repo.ts".to_string(),
        r#"
export function persistCart(cart: any) {
  return { saved: true, cart };
}
"#
        .trim()
        .to_string(),
    );
    ExternalFixtureRepo {
        id: "mutation-impact".to_string(),
        files,
        ground_truth: ExternalGroundTruth {
            task_id: "mutation-impact".to_string(),
            query_text: "Find callers and downstream mutations for addToCart".to_string(),
            expected_files: vec!["src/cart.ts".to_string(), "src/repo.ts".to_string()],
            expected_symbols: vec![
                "checkout".to_string(),
                "addToCart".to_string(),
                "cartState".to_string(),
                "persistCart".to_string(),
            ],
            expected_relation_sequence: vec![
                "CALLS".to_string(),
                "MUTATES".to_string(),
                "CALLS".to_string(),
            ],
            expected_path_symbols: vec![
                "checkout".to_string(),
                "addToCart".to_string(),
                "cartState".to_string(),
                "persistCart".to_string(),
            ],
            critical_source_spans: vec!["src/cart.ts:5-8".to_string()],
            unsupported_fields_allowed: vec!["mutation_relation".to_string()],
        },
    }
}

fn test_impact_fixture() -> ExternalFixtureRepo {
    let mut files = BTreeMap::new();
    files.insert(
        "src/price.ts".to_string(),
        r#"
export function calculateTotal(price: number, tax: number) {
  return price + tax;
}
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "tests/price.test.ts".to_string(),
        r#"
import { calculateTotal } from "../src/price";

test("calculateTotal adds tax", () => {
  expect(calculateTotal(10, 2)).toBe(12);
});
"#
        .trim()
        .to_string(),
    );
    files.insert(
        "tests/name.test.ts".to_string(),
        r#"
test("unrelated name formatting", () => {
  expect("Ada").toBe("Ada");
});
"#
        .trim()
        .to_string(),
    );
    ExternalFixtureRepo {
        id: "test-impact".to_string(),
        files,
        ground_truth: ExternalGroundTruth {
            task_id: "test-impact".to_string(),
            query_text: "Find tests impacted by calculateTotal".to_string(),
            expected_files: vec![
                "src/price.ts".to_string(),
                "tests/price.test.ts".to_string(),
            ],
            expected_symbols: vec!["calculateTotal".to_string()],
            expected_relation_sequence: vec!["TESTS".to_string(), "ASSERTS".to_string()],
            expected_path_symbols: vec![
                "calculateTotal".to_string(),
                "calculateTotal adds tax".to_string(),
            ],
            critical_source_spans: vec![
                "src/price.ts:1-3".to_string(),
                "tests/price.test.ts:3-5".to_string(),
            ],
            unsupported_fields_allowed: vec!["test_relation".to_string()],
        },
    }
}

fn collect_json_evidence(
    value: &Value,
    repo_root: Option<&Path>,
    output: &mut NormalizedCompetitorOutput,
) {
    match value {
        Value::String(text) => collect_text_line_evidence(text, repo_root, output),
        Value::Array(values) => {
            for value in values {
                collect_json_evidence(value, repo_root, output);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                match key.as_str() {
                    "file" | "path" | "file_path" | "relative_path" => {
                        if let Some(text) = value.as_str() {
                            if let Some(path) = normalize_file_path(text, repo_root) {
                                output.files.push(path);
                            }
                        }
                    }
                    "symbol" | "name" | "qualified_name" | "function" | "caller" | "callee" => {
                        if let Some(text) = value.as_str() {
                            push_symbol(text, &mut output.symbols);
                        }
                    }
                    "relation" | "edge_type" | "kind" => {
                        if let Some(text) = value.as_str() {
                            push_relation(text, &mut output.relation_sequence);
                        }
                    }
                    "span" | "source_span" => {
                        if let Some(text) = value.as_str() {
                            push_source_span(text, &mut output.source_spans);
                        }
                    }
                    _ => {}
                }
                collect_json_evidence(value, repo_root, output);
            }
        }
        _ => {}
    }
}

fn collect_text_line_evidence(
    line: &str,
    repo_root: Option<&Path>,
    output: &mut NormalizedCompetitorOutput,
) {
    if should_ignore_text_line(line) {
        return;
    }

    for token in tokenize_for_evidence(line) {
        if let Some(path) = normalize_file_path(&token, repo_root) {
            output.files.push(path.clone());
            if let Some(span) = normalize_source_span(&token, repo_root) {
                output.source_spans.push(span);
            }
            continue;
        }
        if let Some(span) = normalize_source_span(&token, repo_root) {
            output.source_spans.push(span);
            continue;
        }
        if looks_like_relation(&token) {
            push_relation(&token, &mut output.relation_sequence);
            continue;
        }
        if looks_like_symbol(&token) {
            push_symbol(&token, &mut output.symbols);
        }
    }

    if line.contains("->") || line.contains("=>") {
        for token in line
            .replace("=>", "->")
            .split("->")
            .map(|value| value.trim())
        {
            let clean = trim_token(token);
            if looks_like_symbol(&clean) {
                output.path_symbols.push(clean);
            }
        }
    }
}

fn should_ignore_text_line(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return true;
    }
    normalized.starts_with("resolving context")
        || normalized.starts_with("initializing services")
        || normalized.starts_with("services initialized")
        || normalized.starts_with("warning:")
        || normalized.starts_with("no configuration file found")
        || normalized.starts_with("loaded configuration from:")
        || normalized.starts_with("using database:")
        || normalized.starts_with("default_database defined")
        || normalized.starts_with("force re-indexing")
        || normalized.starts_with("deleting existing index")
        || normalized.starts_with("deleted old")
        || normalized.starts_with("welcome to codegraphcontext")
        || normalized.starts_with("cgc organises your code graphs")
        || normalized.starts_with("switch modes anytime")
        || normalized.starts_with("or:")
        || normalized.starts_with("successfully re-indexed:")
        || normalized.starts_with("re-indexing:")
        || normalized.starts_with("no code elements found")
        || normalized.starts_with("no content matches found")
        || normalized.starts_with("no callers found")
        || normalized.starts_with("no function calls found")
        || normalized.starts_with("no call chain found")
}

fn normalize_lists(output: &mut NormalizedCompetitorOutput) {
    dedup(&mut output.files);
    dedup(&mut output.symbols);
    dedup(&mut output.path_symbols);
    dedup(&mut output.relation_sequence);
    dedup(&mut output.source_spans);
    dedup(&mut output.unsupported_fields);
}

fn normalize_file_path(raw: &str, repo_root: Option<&Path>) -> Option<String> {
    let token = trim_token(raw).replace('\\', "/");
    let extensions = [".ts", ".tsx", ".js", ".jsx"];
    let extension_index = extensions
        .iter()
        .filter_map(|extension| token.find(extension).map(|index| index + extension.len()))
        .min()?;
    let mut path = token[..extension_index].to_string();
    if let Some(index) = path.find("src/").or_else(|| path.find("tests/")) {
        path = path[index..].to_string();
    } else if let Some(root) = repo_root {
        let absolute = PathBuf::from(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Ok(stripped) = absolute.strip_prefix(root) {
            path = stripped.display().to_string().replace('\\', "/");
        }
    }
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn normalize_source_span(raw: &str, repo_root: Option<&Path>) -> Option<String> {
    let token = trim_token(raw).replace('\\', "/");
    let path = normalize_file_path(&token, repo_root)?;
    let rest = token.split_once(&path)?.1;
    let span = rest.trim_start_matches(':');
    if span.is_empty() || !span.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let line_part = span
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-' || *ch == ':')
        .collect::<String>();
    Some(format!("{path}:{line_part}"))
}

fn tokenize_for_evidence(line: &str) -> Vec<String> {
    line.split_whitespace()
        .flat_map(|part| part.split(','))
        .map(trim_token)
        .filter(|token| !token.is_empty())
        .collect()
}

fn trim_token(raw: &str) -> String {
    raw.trim_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '"' | '\'' | '`' | ',' | ';' | '[' | ']' | '{' | '}' | '(' | ')' | '<' | '>'
            )
    })
    .trim_end_matches('.')
    .to_string()
}

fn looks_like_symbol(token: &str) -> bool {
    let token = token.trim();
    if token.len() < 2
        || token.contains('/')
        || token.contains('\\')
        || token.contains(':')
        || token.contains(".ts")
        || token.contains(".js")
    {
        return false;
    }
    let mut has_alpha = false;
    for part in token.split('.') {
        if part.is_empty() {
            return false;
        }
        let mut chars = part.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '_' || first.is_ascii_alphabetic()) {
            return false;
        }
        has_alpha = true;
        if !chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()) {
            return false;
        }
    }
    has_alpha && !is_noise_word(token)
}

fn looks_like_relation(token: &str) -> bool {
    let normalized = normalize_relation_name(token);
    matches!(
        normalized.as_str(),
        "CALLS"
            | "READS"
            | "WRITES"
            | "FLOWS_TO"
            | "AUTHORIZES"
            | "CHECKS_ROLE"
            | "PUBLISHES"
            | "EMITS"
            | "CONSUMES"
            | "LISTENS_TO"
            | "MUTATES"
            | "TESTS"
            | "ASSERTS"
            | "EXPOSES"
    )
}

fn push_symbol(raw: &str, symbols: &mut Vec<String>) {
    let clean = trim_token(raw);
    if looks_like_symbol(&clean) {
        symbols.push(clean);
    }
}

fn push_relation(raw: &str, relations: &mut Vec<String>) {
    let relation = normalize_relation_name(raw);
    if !relation.is_empty() {
        relations.push(relation);
    }
}

fn push_source_span(raw: &str, spans: &mut Vec<String>) {
    let clean = trim_token(raw);
    if !clean.is_empty() {
        spans.push(clean.replace('\\', "/"));
    }
}

fn normalize_relation_name(raw: &str) -> String {
    trim_token(raw)
        .replace(['-', '.'], "_")
        .to_ascii_uppercase()
}

fn normalize_relation_names(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| normalize_relation_name(value))
        .collect()
}

fn is_noise_word(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "the"
            | "and"
            | "as"
            | "at"
            | "by"
            | "for"
            | "from"
            | "in"
            | "of"
            | "on"
            | "or"
            | "seconds"
            | "to"
            | "with"
            | "path"
            | "file"
            | "symbol"
            | "symbols"
            | "caller"
            | "callers"
            | "callee"
            | "callees"
            | "query"
            | "result"
            | "results"
            | "match"
            | "matches"
            | "location"
            | "source"
            | "type"
            | "function"
            | "class"
            | "interface"
            | "method"
            | "project"
            | "dependency"
            | "context"
            | "database"
            | "services"
            | "service"
            | "initialized"
            | "connection"
            | "configuration"
            | "defaults"
            | "using"
            | "found"
            | "error"
            | "warning"
            | "defined"
            | "current"
            | "load"
            | "could"
            | "does"
            | "not"
            | "name"
            | "find_dotenv"
            | "kuzudb"
            | "cgc_runtime_db_type"
            | "depth"
            | "between"
            | "within"
            | "true"
            | "false"
            | "null"
            | "undefined"
    )
}

fn false_proof_count(truth: &ExternalGroundTruth, normalized: &NormalizedCompetitorOutput) -> u64 {
    if normalized.mode == NormalizationMode::UnsupportedOrUnparseable {
        return 0;
    }
    let expected_relations = set_from(&normalize_relation_names(&truth.expected_relation_sequence));
    let observed_relations = set_from(&normalize_relation_names(&normalized.relation_sequence));
    observed_relations
        .difference(&expected_relations)
        .filter(|relation| !relation.is_empty())
        .count() as u64
}

fn compact_entity_id(id: &str) -> String {
    let after_hash = id.split('#').next_back().unwrap_or(id);
    let before_paren = after_hash.split('(').next().unwrap_or(after_hash);
    let before_at = before_paren.split('@').next().unwrap_or(before_paren);
    let candidate = before_at
        .split(':')
        .rfind(|part| !part.is_empty())
        .unwrap_or(before_at)
        .split('.')
        .rfind(|part| !part.is_empty())
        .unwrap_or(before_at)
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$');
    if candidate.is_empty() {
        id.to_string()
    } else {
        candidate.to_string()
    }
}

fn aggregate_external_runs(
    runs: &[ExternalComparisonRun],
) -> BTreeMap<String, ExternalComparisonAggregate> {
    let mut grouped: BTreeMap<String, Vec<&ExternalComparisonRun>> = BTreeMap::new();
    for run in runs {
        grouped
            .entry(run.mode.as_str().to_string())
            .or_default()
            .push(run);
    }
    grouped
        .into_iter()
        .map(|(mode, runs)| {
            let completed = runs
                .iter()
                .copied()
                .filter(|run| run.status == ExternalComparisonStatus::Completed)
                .collect::<Vec<_>>();
            let divisor = completed.len().max(1) as f64;
            let aggregate = ExternalComparisonAggregate {
                runs: completed.len(),
                skipped: runs
                    .iter()
                    .filter(|run| run.status == ExternalComparisonStatus::Skipped)
                    .count(),
                average_file_recall_at_10: average_metric(
                    &completed,
                    |run| metric_at(&run.metrics.file_recall_at_k, "10"),
                    divisor,
                ),
                average_symbol_recall_at_10: average_metric(
                    &completed,
                    |run| metric_at(&run.metrics.symbol_recall_at_k, "10"),
                    divisor,
                ),
                average_path_recall_at_10: average_metric(
                    &completed,
                    |run| metric_at(&run.metrics.path_recall_at_k, "10"),
                    divisor,
                ),
                average_relation_f1: average_metric(
                    &completed,
                    |run| run.metrics.relation_f1,
                    divisor,
                ),
                average_mrr: average_metric(&completed, |run| run.metrics.mrr, divisor),
                average_ndcg: average_metric(&completed, |run| run.metrics.ndcg, divisor),
                unsupported_feature_count: runs
                    .iter()
                    .map(|run| run.metrics.unsupported_feature_count)
                    .sum(),
                false_proof_count: runs.iter().map(|run| run.metrics.false_proof_count).sum(),
                total_index_latency_ms: completed
                    .iter()
                    .map(|run| run.metrics.index_latency_ms)
                    .sum(),
                total_query_latency_ms: completed
                    .iter()
                    .map(|run| run.metrics.query_latency_ms)
                    .sum(),
            };
            (mode, aggregate)
        })
        .collect()
}

fn average_metric(
    runs: &[&ExternalComparisonRun],
    value: impl Fn(&ExternalComparisonRun) -> f64,
    divisor: f64,
) -> f64 {
    if runs.is_empty() {
        0.0
    } else {
        runs.iter().map(|run| value(run)).sum::<f64>() / divisor
    }
}

fn metric_at(map: &BTreeMap<String, f64>, key: &str) -> f64 {
    map.get(key).copied().unwrap_or(0.0)
}

fn write_external_report(report: &ExternalComparisonReport, report_dir: &Path) -> BenchResult<()> {
    write_json_file(&report_dir.join("run.json"), report)?;

    let jsonl = report
        .runs
        .iter()
        .map(|run| {
            serde_json::to_string(run).map_err(|error| BenchmarkError::Parse(error.to_string()))
        })
        .collect::<BenchResult<Vec<_>>>()?
        .join("\n");
    fs::write(report_dir.join("per_task.jsonl"), format!("{jsonl}\n"))?;
    fs::write(
        report_dir.join("summary.md"),
        render_external_markdown(report),
    )?;
    Ok(())
}

fn render_external_markdown(report: &ExternalComparisonReport) -> String {
    let mut output = format!(
        "# CodeGraphContext External Comparison\n\nBenchmark: `{}`\n\nCompetitor: `{}` from `{}`\n\nExecutable: `{}`\n\n| Mode | Runs | Skipped | File R@10 | Symbol R@10 | Path R@10 | Relation F1 | MRR | NDCG | Unsupported | False Proofs |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
        report.benchmark_id,
        report.manifest.competitor_name,
        report.manifest.source_repo_url,
        report.manifest.executable_used
    );
    for (mode, aggregate) in &report.aggregate {
        output.push_str(&format!(
            "| `{}` | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} |\n",
            mode,
            aggregate.runs,
            aggregate.skipped,
            aggregate.average_file_recall_at_10,
            aggregate.average_symbol_recall_at_10,
            aggregate.average_path_recall_at_10,
            aggregate.average_relation_f1,
            aggregate.average_mrr,
            aggregate.average_ndcg,
            aggregate.unsupported_feature_count,
            aggregate.false_proof_count
        ));
    }
    output.push_str(
        "\nUnsupported or unparseable competitor fields are counted separately from incorrect results. No SOTA claim is implied by this report.\n",
    );
    output
}

fn write_json_file(path: &Path, value: &impl Serialize) -> BenchResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(value)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
    )?;
    Ok(())
}

fn fairness_rules() -> Vec<String> {
    vec![
        "same fixture repo".to_string(),
        "same query wording".to_string(),
        "fresh index per run unless a warm-cache mode is explicitly labeled".to_string(),
        format!("same top-k default: {DEFAULT_TOP_K}"),
        format!("same timeout default: {DEFAULT_TIMEOUT_MS} ms"),
        "no network during benchmark execution".to_string(),
        "machine-readable reports distinguish unsupported capability from incorrect result"
            .to_string(),
        "no SOTA superiority claim without measured evidence".to_string(),
    ]
}

fn skipped_manifest(reason: &str) -> CompetitorManifest {
    CompetitorManifest {
        competitor_name: COMPETITOR_NAME.to_string(),
        source_repo_url: COMPETITOR_REPO_URL.to_string(),
        pinned_git_commit_sha: env::var("CGC_COMPETITOR_COMMIT")
            .unwrap_or_else(|_| "unknown".to_string()),
        detected_package_version: "unknown".to_string(),
        observed_version_hint: OBSERVED_VERSION.to_string(),
        python_version: detect_python_version(),
        executable_used: format!("skipped: {reason}"),
        detected_database_backend: competitor_backend(),
        install_mode: env::var("CGC_COMPETITOR_INSTALL_MODE")
            .unwrap_or_else(|_| "unknown".to_string()),
        benchmark_run_timestamp_unix_ms: unix_ms(),
        host_platform: HostPlatformMetadata::current(),
    }
}

fn competitor_backend() -> String {
    env::var(COMPETITOR_BACKEND_ENV).unwrap_or_else(|_| DEFAULT_CGC_DATABASE_BACKEND.to_string())
}

fn absolute_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn detect_python_version_for_executable(executable: &Path) -> String {
    let Some(parent) = executable.parent() else {
        return detect_python_version();
    };
    let candidate = if cfg!(windows) {
        parent.join("python.exe")
    } else {
        parent.join("python")
    };
    if candidate.is_file() {
        if let Some(version) = run_python_version_command(&candidate) {
            return version;
        }
    }
    detect_python_version()
}

fn detect_python_version() -> String {
    for candidate in ["python", "python3", "py"] {
        if let Some(version) = run_python_version_command(Path::new(candidate)) {
            return version;
        }
    }
    "unknown".to_string()
}

fn run_python_version_command(command: &Path) -> Option<String> {
    let output = Command::new(command)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_version_from_text(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|token| token.chars().any(|ch| ch.is_ascii_digit()) && token.contains('.'))
        .map(trim_token)
}

fn find_executable_on_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    let extensions = if cfg!(windows) {
        vec![".exe", ".cmd", ".bat", ""]
    } else {
        vec![""]
    };
    for dir in env::split_paths(&path_var) {
        for extension in &extensions {
            let candidate = dir.join(format!("{name}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn estimate_token_cost(query: &str, output: &NormalizedCompetitorOutput) -> u64 {
    let bytes = query.len()
        + output.files.iter().map(String::len).sum::<usize>()
        + output.symbols.iter().map(String::len).sum::<usize>()
        + output.path_symbols.iter().map(String::len).sum::<usize>();
    bytes.div_ceil(4) as u64
}

fn set_from(values: &[String]) -> BTreeSet<String> {
    values.iter().cloned().collect()
}

fn dedup(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unique_output_path;

    #[test]
    fn competitor_manifest_validation_accepts_required_metadata() {
        let manifest = skipped_manifest("not installed");
        manifest.validate().expect("manifest validates");
        assert_eq!(manifest.source_repo_url, COMPETITOR_REPO_URL);
        assert_eq!(manifest.observed_version_hint, OBSERVED_VERSION);
    }

    #[test]
    fn external_fixture_ground_truth_validates() {
        let fixtures = external_competitor_fixtures();
        assert_eq!(fixtures.len(), 5);
        for fixture in fixtures {
            fixture.validate().expect("fixture validates");
            assert!(fixture
                .ground_truth
                .unsupported_fields_allowed
                .iter()
                .any(|field| {
                    field.contains("relation")
                        || field.contains("mutation")
                        || field.contains("test")
                }));
        }
    }

    #[test]
    fn normalization_extracts_conservative_evidence_from_human_output() {
        let output = normalize_codegraphcontext_text(
            r#"
Symbol: postInvoice src/api.ts:3-5
Path: postInvoice -> createInvoice -> saveInvoice -> writeDatabase
Relations: CALLS -> CALLS -> WRITES
"#,
            "",
            None,
        );
        assert_eq!(output.mode, NormalizationMode::HumanText);
        assert!(output.files.contains(&"src/api.ts".to_string()));
        assert!(output.symbols.contains(&"postInvoice".to_string()));
        assert!(output.path_symbols.contains(&"writeDatabase".to_string()));
        assert!(output.relation_sequence.contains(&"CALLS".to_string()));
        assert!(output.source_spans.contains(&"src/api.ts:3-5".to_string()));
    }

    #[test]
    fn normalization_ignores_cgc_boilerplate_and_negative_results() {
        let output = normalize_codegraphcontext_text(
            r#"
Resolving context...
Initializing services and database connection...
Services initialized.
No code elements found with name 'adminRoute'
No callers found for 'adminRoute'
No function calls found for 'adminRoute'
No call chain found between 'adminRoute' and 'checkRole' within depth 10
"#,
            "Warning: Could not load .env from current directory: name 'find_dotenv' is not defined",
            None,
        );
        assert_eq!(output.mode, NormalizationMode::UnsupportedOrUnparseable);
        assert!(!output.symbols.contains(&"adminRoute".to_string()));
        assert!(output.relation_sequence.is_empty());
    }

    #[test]
    fn external_metric_calculation_counts_unsupported_separately() {
        let fixture = ts_call_chain_fixture();
        let normalized = NormalizedCompetitorOutput {
            files: vec!["src/api.ts".to_string(), "src/service.ts".to_string()],
            symbols: vec!["postInvoice".to_string()],
            path_symbols: vec!["postInvoice".to_string(), "createInvoice".to_string()],
            relation_sequence: vec!["CALLS".to_string()],
            source_spans: Vec::new(),
            unsupported_fields: vec!["source_span_coverage".to_string()],
            mode: NormalizationMode::HumanText,
            warnings: Vec::new(),
            raw_json: None,
        };
        let metrics = calculate_external_metrics(&fixture.ground_truth, &normalized, 10, 12, 34);
        assert!(metrics.file_recall_at_k["10"] > 0.0);
        assert_eq!(metrics.unsupported_feature_count, 1);
        assert_eq!(metrics.index_latency_ms, 12);
        assert_eq!(metrics.query_latency_ms, 34);
    }

    #[test]
    fn skipped_run_behavior_when_competitor_executable_is_unavailable() {
        let fixture = ts_call_chain_fixture();
        let run = skipped_external_run(&fixture, "missing executable");
        assert_eq!(run.status, ExternalComparisonStatus::Skipped);
        assert_eq!(
            run.normalized_output.mode,
            NormalizationMode::UnsupportedOrUnparseable
        );
        assert!(run.skip_reason.expect("skip reason").contains("missing"));
    }

    #[test]
    fn repo_local_discovery_candidates_include_recovery_and_legacy_venvs() {
        let candidates = repo_local_cgc_candidate_paths()
            .into_iter()
            .map(|path| path.display().to_string().replace('\\', "/"))
            .collect::<Vec<_>>();
        assert!(candidates
            .iter()
            .any(|path| path.ends_with(".tools/cgc_recovery/venv312_compat/Scripts/cgc.exe")));
        assert!(candidates
            .iter()
            .any(|path| path.ends_with("target/cgc-competitor/.venv312-pypi/Scripts/cgc.exe")));
        assert!(default_repo_local_cgc_home()
            .display()
            .to_string()
            .replace('\\', "/")
            .ends_with(".tools/cgc_recovery/home_shared"));
    }

    #[test]
    fn external_report_output_writes_expected_files_without_competitor() {
        let root = unique_output_path("cgc-comparison-report", "dir");
        let report = run_codegraphcontext_comparison(CodeGraphContextComparisonOptions {
            report_dir: root.clone(),
            timeout_ms: 25,
            top_k: 10,
            competitor_executable: Some(root.join("missing-cgc.exe")),
        })
        .expect("report writes");

        assert!(root.join("run.json").exists());
        assert!(root.join("per_task.jsonl").exists());
        assert!(root.join("summary.md").exists());
        assert!(root.join("normalized_outputs").join("codegraph").exists());
        assert!(report
            .aggregate
            .contains_key(ExternalComparisonMode::CodeGraphContextCli.as_str()));
        assert!(report
            .runs
            .iter()
            .any(|run| run.status == ExternalComparisonStatus::Skipped));

        fs::remove_dir_all(root).expect("cleanup report");
    }

    #[test]
    #[ignore = "requires CGC_COMPETITOR_BIN pointing at a local CodeGraphContext executable"]
    fn live_codegraphcontext_smoke_when_configured() {
        let Some(raw) = env::var_os(COMPETITOR_BIN_ENV) else {
            return;
        };
        let runner = CodeGraphContextRunner::with_executable(PathBuf::from(raw), 30_000);
        let fixture = ts_call_chain_fixture();
        let root = unique_output_path("cgc-live-smoke", "dir");
        fixture.write_to(&root).expect("write live fixture");
        let index = runner.index_repo(&root);
        let symbol = runner.symbol_search(&root, "postInvoice", 10);
        let normalized = normalize_codegraphcontext_text(
            &format!("{}\n{}", index.stdout, symbol.stdout),
            &format!("{}\n{}", index.stderr, symbol.stderr),
            Some(&root),
        );
        assert!(
            index.exit_code == Some(0) || symbol.exit_code == Some(0),
            "index={index:?} symbol={symbol:?} normalized={normalized:?}"
        );
        fs::remove_dir_all(root).expect("cleanup live fixture");
    }
}
