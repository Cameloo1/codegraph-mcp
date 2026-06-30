//! Benchmark Layer v1 real-agent patch task registry contract.
//!
//! This module extends the existing benchmark task concepts for the v1
//! same-agent patch-outcome layer. It defines the provider-visible/evaluator-only
//! split, pinned dataset manifest, and local fixture checks; it does not run a
//! real external-agent benchmark.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    default_k_values, BenchResult, BenchmarkError, BenchmarkFamily, BenchmarkRepoSpec,
    BenchmarkTask, GroundTruth, SyntheticRepoKind,
};

pub const V1_TASK_REGISTRY_SCHEMA_VERSION: u32 = 1;

pub const REQUIRED_V1_TASK_FAMILIES: [&str; 14] = [
    "deterministic_local_patch_fixtures",
    "hallucination_trap_fixtures",
    "wrong_file_trap_fixtures",
    "nonexistent_symbol_trap_fixtures",
    "stale_docs_evidence_fixtures",
    "same_name_ambiguity_fixtures",
    "config_driven_behavior_fixtures",
    "deleted_renamed_symbol_fixtures",
    "test_mock_leakage_fixtures",
    "simple_multi_file_patch_fixtures",
    "rtds_validate_update_fixtures",
    "swebench_lite_focused_tasks",
    "swebench_lite_ladders_1_5_10",
    "swebench_verified_pro_future_blocked",
];

const ALLOWED_DATASET_STATUSES: [&str; 5] = [
    "ready",
    "blocked_external",
    "blocked_harness_missing",
    "not_configured",
    "future_blocked",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1PatchTaskRegistry {
    pub schema_version: u32,
    pub registry_id: String,
    pub generated_for_phase: String,
    pub public_claim: bool,
    pub required_task_families: Vec<String>,
    pub task_family_coverage: BTreeMap<String, V1TaskFamilyCoverage>,
    pub tasks: Vec<V1PatchTask>,
    pub pinned_datasets: V1PinnedDatasetManifest,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl V1PatchTaskRegistry {
    pub fn validate(&self) -> BenchResult<()> {
        if self.schema_version != V1_TASK_REGISTRY_SCHEMA_VERSION {
            return Err(BenchmarkError::Validation(format!(
                "expected v1 task registry schema {}, got {}",
                V1_TASK_REGISTRY_SCHEMA_VERSION, self.schema_version
            )));
        }
        if self.registry_id.trim().is_empty() || self.generated_for_phase.trim().is_empty() {
            return Err(BenchmarkError::Validation(
                "registry id and phase must not be empty".to_string(),
            ));
        }
        if self.public_claim {
            return Err(BenchmarkError::Validation(
                "v1 task registry must not allow public claims".to_string(),
            ));
        }
        self.validate_required_family_coverage()?;
        self.validate_task_dedupe()?;
        for task in &self.tasks {
            task.validate()?;
            task.to_benchmark_task().validate()?;
        }
        self.leakage_audit().validate()?;
        self.pinned_datasets
            .validate(&self.tasks.iter().map(|task| task.task_id.clone()).collect())?;
        Ok(())
    }

    pub fn validate_fixture_checkouts(&self, workspace_root: &Path) -> BenchResult<()> {
        for task in self
            .tasks
            .iter()
            .filter(|task| task.repo_source == "local_fixture")
        {
            task.validate_fixture_checkout(workspace_root)?;
        }
        Ok(())
    }

    pub fn agent_visible_tasks(&self) -> Vec<V1AgentVisibleTask> {
        self.tasks.iter().map(V1PatchTask::agent_visible).collect()
    }

    pub fn leakage_audit(&self) -> V1LeakageAudit {
        let mut violations = Vec::new();
        for task in &self.tasks {
            violations.extend(task.hidden_gold_leakage_violations());
        }
        V1LeakageAudit {
            passed: violations.is_empty(),
            violation_count: violations.len() as u64,
            violations,
        }
    }

    fn validate_required_family_coverage(&self) -> BenchResult<()> {
        let required = REQUIRED_V1_TASK_FAMILIES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let listed = self
            .required_task_families
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if listed != required {
            return Err(BenchmarkError::Validation(
                "required v1 task family list does not match contract".to_string(),
            ));
        }
        for family in required {
            let coverage = self.task_family_coverage.get(family).ok_or_else(|| {
                BenchmarkError::Validation(format!("missing task family coverage for {family}"))
            })?;
            coverage.validate(family)?;
        }
        Ok(())
    }

    fn validate_task_dedupe(&self) -> BenchResult<()> {
        if self.tasks.is_empty() {
            return Err(BenchmarkError::Validation(
                "v1 task registry must contain tasks".to_string(),
            ));
        }
        let mut seen = BTreeSet::new();
        for task in &self.tasks {
            if !seen.insert(task.task_id.as_str()) {
                return Err(BenchmarkError::Validation(format!(
                    "duplicate v1 task id {}",
                    task.task_id
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1TaskFamilyCoverage {
    pub status: String,
    #[serde(default)]
    pub task_ids: Vec<String>,
    pub validation_rule: String,
    pub repair_action_if_missing: String,
}

impl V1TaskFamilyCoverage {
    fn validate(&self, family: &str) -> BenchResult<()> {
        if self.status.trim().is_empty()
            || self.validation_rule.trim().is_empty()
            || self.repair_action_if_missing.trim().is_empty()
        {
            return Err(BenchmarkError::Validation(format!(
                "task family {family} coverage must include status, validation rule, and repair action"
            )));
        }
        if self.status == "ready" && self.task_ids.is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "ready task family {family} must name task ids"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1PatchTask {
    pub task_id: String,
    pub task_family: String,
    pub dataset_name: String,
    pub repo_source: String,
    pub repo_commit: String,
    pub checkout_path_policy: String,
    pub issue_text: String,
    pub task_prompt: String,
    pub visible_query_terms: Vec<String>,
    pub visible_file_hints: Vec<String>,
    pub visible_symbol_hints: Vec<String>,
    pub visible_error_messages: Vec<String>,
    pub hidden_gold_files: Vec<String>,
    pub hidden_gold_symbols: Vec<String>,
    pub forbidden_files: Vec<String>,
    pub forbidden_symbols: Vec<String>,
    pub expected_touched_files: Vec<String>,
    pub expected_tests: Vec<String>,
    pub setup_command: String,
    pub test_command: String,
    pub evaluator_kind: String,
    pub timeout_ms: u64,
    pub token_budget: u64,
    pub tool_budget: u64,
    pub docker_image_or_env: String,
    pub patch_apply_policy: String,
    pub no_gold_leakage_expected: bool,
    pub codegraph_attribution_criteria: V1CodeGraphAttributionCriteria,
    pub proof_boundary_expectations: V1ProofBoundaryExpectations,
    pub public_claim_allowed: bool,
    pub status: String,
    pub fixture_repo_path: Option<String>,
}

impl V1PatchTask {
    pub fn validate(&self) -> BenchResult<()> {
        for (field, value) in [
            ("task_id", &self.task_id),
            ("task_family", &self.task_family),
            ("dataset_name", &self.dataset_name),
            ("repo_source", &self.repo_source),
            ("repo_commit", &self.repo_commit),
            ("checkout_path_policy", &self.checkout_path_policy),
            ("issue_text", &self.issue_text),
            ("task_prompt", &self.task_prompt),
            ("setup_command", &self.setup_command),
            ("test_command", &self.test_command),
            ("evaluator_kind", &self.evaluator_kind),
            ("docker_image_or_env", &self.docker_image_or_env),
            ("patch_apply_policy", &self.patch_apply_policy),
            ("status", &self.status),
        ] {
            if value.trim().is_empty() {
                return Err(BenchmarkError::Validation(format!(
                    "task {} missing required field {field}",
                    self.task_id
                )));
            }
        }
        if !self.no_gold_leakage_expected {
            return Err(BenchmarkError::Validation(format!(
                "task {} must set no_gold_leakage_expected=true",
                self.task_id
            )));
        }
        if self.public_claim_allowed {
            return Err(BenchmarkError::Validation(format!(
                "task {} must not allow public claims",
                self.task_id
            )));
        }
        if self.timeout_ms == 0 || self.token_budget == 0 || self.tool_budget == 0 {
            return Err(BenchmarkError::Validation(format!(
                "task {} must define positive timeout/token/tool budgets",
                self.task_id
            )));
        }
        if self.hidden_gold_files.is_empty()
            && self.hidden_gold_symbols.is_empty()
            && self.expected_touched_files.is_empty()
        {
            return Err(BenchmarkError::Validation(format!(
                "task {} must include evaluator-only gold or expected touched files",
                self.task_id
            )));
        }
        self.codegraph_attribution_criteria
            .validate(&self.task_id)?;
        self.proof_boundary_expectations.validate(&self.task_id)?;
        Ok(())
    }

    pub fn agent_visible(&self) -> V1AgentVisibleTask {
        V1AgentVisibleTask {
            task_id: self.task_id.clone(),
            task_family: self.task_family.clone(),
            dataset_name: self.dataset_name.clone(),
            repo_source: self.repo_source.clone(),
            repo_commit: self.repo_commit.clone(),
            checkout_path_policy: self.checkout_path_policy.clone(),
            issue_text: self.issue_text.clone(),
            task_prompt: self.task_prompt.clone(),
            visible_query_terms: self.visible_query_terms.clone(),
            visible_file_hints: self.visible_file_hints.clone(),
            visible_symbol_hints: self.visible_symbol_hints.clone(),
            visible_error_messages: self.visible_error_messages.clone(),
            setup_command: self.setup_command.clone(),
            test_command: self.test_command.clone(),
            timeout_ms: self.timeout_ms,
            token_budget: self.token_budget,
            tool_budget: self.tool_budget,
            docker_image_or_env: self.docker_image_or_env.clone(),
            patch_apply_policy: self.patch_apply_policy.clone(),
        }
    }

    pub fn to_benchmark_task(&self) -> BenchmarkTask {
        BenchmarkTask {
            id: self.task_id.clone(),
            family: BenchmarkFamily::AgentPatch,
            prompt: self.task_prompt.clone(),
            repo: BenchmarkRepoSpec::Synthetic {
                kind: SyntheticRepoKind::AgentPatch,
            },
            ground_truth: GroundTruth {
                expected_files: self.hidden_gold_files.clone(),
                expected_symbols: self.hidden_gold_symbols.clone(),
                expected_tests: self.expected_tests.clone(),
                expected_patch_success: Some(self.status == "ready"),
                expected_test_success: Some(self.status == "ready"),
                metadata: BTreeMap::from([
                    (
                        "task_family".to_string(),
                        Value::String(self.task_family.clone()),
                    ),
                    (
                        "evaluator_kind".to_string(),
                        Value::String(self.evaluator_kind.clone()),
                    ),
                    (
                        "forbidden_files".to_string(),
                        Value::Array(
                            self.forbidden_files
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    ),
                    (
                        "forbidden_symbols".to_string(),
                        Value::Array(
                            self.forbidden_symbols
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    ),
                ]),
                ..GroundTruth::default()
            },
            k_values: default_k_values(),
            metadata: BTreeMap::from([
                (
                    "v1_schema_extension".to_string(),
                    Value::String("benchmark_layer_v1_patch_task_contract".to_string()),
                ),
                (
                    "public_claim_allowed".to_string(),
                    Value::Bool(self.public_claim_allowed),
                ),
            ]),
        }
    }

    fn hidden_gold_leakage_violations(&self) -> Vec<V1LeakageViolation> {
        let mut violations = Vec::new();
        let prompt_text = format!("{}\n{}", self.issue_text, self.task_prompt);
        let visible_fields = [
            ("visible_query_terms", self.visible_query_terms.join("\n")),
            ("visible_file_hints", self.visible_file_hints.join("\n")),
            ("visible_symbol_hints", self.visible_symbol_hints.join("\n")),
            (
                "visible_error_messages",
                self.visible_error_messages.join("\n"),
            ),
            ("setup_command", self.setup_command.clone()),
            ("test_command", self.test_command.clone()),
        ];
        for gold in self
            .hidden_gold_files
            .iter()
            .chain(self.hidden_gold_symbols.iter())
            .filter(|gold| !gold.trim().is_empty())
        {
            let prompt_allows = contains_case_insensitive(&prompt_text, gold);
            for (field, value) in &visible_fields {
                if contains_case_insensitive(value, gold) && !prompt_allows {
                    violations.push(V1LeakageViolation {
                        task_id: self.task_id.clone(),
                        hidden_value: gold.clone(),
                        visible_field: (*field).to_string(),
                    });
                }
            }
        }
        violations
    }

    fn validate_fixture_checkout(&self, workspace_root: &Path) -> BenchResult<()> {
        let fixture_repo_path = self.fixture_repo_path.as_deref().ok_or_else(|| {
            BenchmarkError::Validation(format!(
                "local fixture task {} missing fixture_repo_path",
                self.task_id
            ))
        })?;
        let repo_root = workspace_root.join(fixture_repo_path);
        if !repo_root.is_dir() {
            return Err(BenchmarkError::Validation(format!(
                "local fixture task {} repo path does not exist: {}",
                self.task_id,
                repo_root.display()
            )));
        }
        for relative_path in self
            .hidden_gold_files
            .iter()
            .chain(self.forbidden_files.iter())
            .chain(self.expected_touched_files.iter())
            .chain(self.expected_tests.iter())
            .filter(|path| !path.trim().is_empty())
        {
            let path = repo_root.join(relative_path);
            if !path.is_file() {
                return Err(BenchmarkError::Validation(format!(
                    "local fixture task {} missing fixture file {}",
                    self.task_id, relative_path
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1AgentVisibleTask {
    pub task_id: String,
    pub task_family: String,
    pub dataset_name: String,
    pub repo_source: String,
    pub repo_commit: String,
    pub checkout_path_policy: String,
    pub issue_text: String,
    pub task_prompt: String,
    pub visible_query_terms: Vec<String>,
    pub visible_file_hints: Vec<String>,
    pub visible_symbol_hints: Vec<String>,
    pub visible_error_messages: Vec<String>,
    pub setup_command: String,
    pub test_command: String,
    pub timeout_ms: u64,
    pub token_budget: u64,
    pub tool_budget: u64,
    pub docker_image_or_env: String,
    pub patch_apply_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1CodeGraphAttributionCriteria {
    pub codegraph_available_only_in_b_arm: bool,
    pub requires_release_binary: bool,
    pub requires_external_profile_db: bool,
    pub requires_fresh_context: bool,
    pub attribution_invalid_when_context_unavailable_or_stale: bool,
    pub attribution_invalid_when_patch_not_aligned_with_context: bool,
}

impl V1CodeGraphAttributionCriteria {
    fn validate(&self, task_id: &str) -> BenchResult<()> {
        if !self.codegraph_available_only_in_b_arm
            || !self.requires_release_binary
            || !self.requires_external_profile_db
            || !self.requires_fresh_context
            || !self.attribution_invalid_when_context_unavailable_or_stale
            || !self.attribution_invalid_when_patch_not_aligned_with_context
        {
            return Err(BenchmarkError::Validation(format!(
                "task {task_id} has weak CodeGraph attribution criteria"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1ProofBoundaryExpectations {
    pub graph_source_verification_only_graph_proof: bool,
    pub text_evidence_not_graph_proof: bool,
    pub candidate_evidence_not_graph_proof: bool,
    pub patch_success_not_codegraph_attribution: bool,
    pub no_proof_path_status_allowed: bool,
}

impl V1ProofBoundaryExpectations {
    fn validate(&self, task_id: &str) -> BenchResult<()> {
        if !self.graph_source_verification_only_graph_proof
            || !self.text_evidence_not_graph_proof
            || !self.candidate_evidence_not_graph_proof
            || !self.patch_success_not_codegraph_attribution
        {
            return Err(BenchmarkError::Validation(format!(
                "task {task_id} has weak proof-boundary expectations"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1PinnedDatasetManifest {
    pub schema_version: u32,
    pub datasets: Vec<V1PinnedDataset>,
}

impl V1PinnedDatasetManifest {
    fn validate(&self, task_ids: &BTreeSet<String>) -> BenchResult<()> {
        if self.schema_version != V1_TASK_REGISTRY_SCHEMA_VERSION {
            return Err(BenchmarkError::Validation(format!(
                "expected pinned dataset schema {}, got {}",
                V1_TASK_REGISTRY_SCHEMA_VERSION, self.schema_version
            )));
        }
        if self.datasets.is_empty() {
            return Err(BenchmarkError::Validation(
                "pinned dataset manifest must include datasets".to_string(),
            ));
        }
        let mut seen = BTreeSet::new();
        for dataset in &self.datasets {
            dataset.validate(task_ids)?;
            if !seen.insert(dataset.dataset_name.as_str()) {
                return Err(BenchmarkError::Validation(format!(
                    "duplicate pinned dataset {}",
                    dataset.dataset_name
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1PinnedDataset {
    pub dataset_name: String,
    pub upstream_repo_url: String,
    pub pinned_commit_or_hash: String,
    pub task_ids: Vec<String>,
    pub subset_selection_method: String,
    pub local_checkout_path_policy: String,
    pub data_available_in_this_checkout: bool,
    pub raw_dataset_artifacts_ignored_local: bool,
    pub status: String,
    pub producing_prompt_or_report: String,
    pub validation_rule: String,
    pub repair_action_if_missing: String,
}

impl V1PinnedDataset {
    fn validate(&self, task_ids: &BTreeSet<String>) -> BenchResult<()> {
        for (field, value) in [
            ("dataset_name", &self.dataset_name),
            ("upstream_repo_url", &self.upstream_repo_url),
            ("pinned_commit_or_hash", &self.pinned_commit_or_hash),
            ("subset_selection_method", &self.subset_selection_method),
            (
                "local_checkout_path_policy",
                &self.local_checkout_path_policy,
            ),
            ("status", &self.status),
            (
                "producing_prompt_or_report",
                &self.producing_prompt_or_report,
            ),
            ("validation_rule", &self.validation_rule),
            ("repair_action_if_missing", &self.repair_action_if_missing),
        ] {
            if value.trim().is_empty() {
                return Err(BenchmarkError::Validation(format!(
                    "dataset {} missing required field {field}",
                    self.dataset_name
                )));
            }
        }
        if !ALLOWED_DATASET_STATUSES.contains(&self.status.as_str()) {
            return Err(BenchmarkError::Validation(format!(
                "dataset {} has unsupported status {}",
                self.dataset_name, self.status
            )));
        }
        if !self.raw_dataset_artifacts_ignored_local {
            return Err(BenchmarkError::Validation(format!(
                "dataset {} must keep raw artifacts ignored/local",
                self.dataset_name
            )));
        }
        for task_id in &self.task_ids {
            if !task_ids.contains(task_id) {
                return Err(BenchmarkError::Validation(format!(
                    "dataset {} references unknown task {}",
                    self.dataset_name, task_id
                )));
            }
        }
        if !self.data_available_in_this_checkout && self.status == "ready" {
            return Err(BenchmarkError::Validation(format!(
                "dataset {} cannot be ready when data is unavailable",
                self.dataset_name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1LeakageAudit {
    pub passed: bool,
    pub violation_count: u64,
    pub violations: Vec<V1LeakageViolation>,
}

impl V1LeakageAudit {
    fn validate(&self) -> BenchResult<()> {
        if !self.passed || self.violation_count != 0 || !self.violations.is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "gold leakage audit failed with {} violations",
                self.violation_count
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1LeakageViolation {
    pub task_id: String,
    pub hidden_value: String,
    pub visible_field: String,
}

pub fn load_v1_patch_task_registry(path: &Path) -> BenchResult<V1PatchTaskRegistry> {
    let raw = fs::read_to_string(path)?;
    let registry: V1PatchTaskRegistry =
        serde_json::from_str(&raw).map_err(|error| BenchmarkError::Parse(error.to_string()))?;
    registry.validate()?;
    Ok(registry)
}

pub fn validate_v1_patch_task_registry_json(value: &Value) -> BenchResult<V1PatchTaskRegistry> {
    let registry: V1PatchTaskRegistry = serde_json::from_value(value.clone())
        .map_err(|error| BenchmarkError::Parse(error.to_string()))?;
    registry.validate()?;
    Ok(registry)
}

pub fn v1_registry_path(workspace_root: &Path) -> PathBuf {
    workspace_root
        .join("benchmarks")
        .join("datasets")
        .join("v1_real_agent_patch_outcomes")
        .join("task_registry.json")
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf()
    }

    fn registry() -> V1PatchTaskRegistry {
        let path = v1_registry_path(&workspace_root());
        load_v1_patch_task_registry(&path).expect("v1 registry loads")
    }

    #[test]
    fn v1_task_registry_loads_and_validates() {
        let registry = registry();
        assert_eq!(registry.schema_version, V1_TASK_REGISTRY_SCHEMA_VERSION);
        assert!(!registry.public_claim);
        assert!(registry.tasks.len() >= 10);
        assert!(registry.tasks.iter().all(|task| !task.public_claim_allowed));
    }

    #[test]
    fn v1_agent_visible_task_strips_hidden_evaluator_fields() {
        let visible = registry().agent_visible_tasks();
        let raw = serde_json::to_value(&visible[0]).expect("visible task serializes");
        assert!(raw.get("hidden_gold_files").is_none());
        assert!(raw.get("hidden_gold_symbols").is_none());
        assert!(raw.get("forbidden_files").is_none());
        assert!(raw.get("forbidden_symbols").is_none());
        assert!(raw.get("expected_touched_files").is_none());
        assert!(raw.get("expected_tests").is_none());
    }

    #[test]
    fn v1_gold_leakage_audit_passes_for_registry() {
        let audit = registry().leakage_audit();
        assert!(audit.passed);
        assert_eq!(audit.violation_count, 0);
    }

    #[test]
    fn v1_gold_leakage_audit_fails_hidden_value_in_visible_field() {
        let mut registry = registry();
        let hidden = registry.tasks[0].hidden_gold_files[0].clone();
        registry.tasks[0].visible_file_hints.push(hidden.clone());
        let audit = registry.leakage_audit();
        assert!(!audit.passed);
        assert_eq!(audit.violations[0].hidden_value, hidden);
    }

    #[test]
    fn v1_gold_leakage_audit_allows_prompt_explicit_hidden_value() {
        let mut registry = registry();
        let hidden = registry.tasks[0].hidden_gold_files[0].clone();
        registry.tasks[0]
            .task_prompt
            .push_str(&format!(" The task prompt explicitly names {hidden}."));
        registry.tasks[0].visible_file_hints.push(hidden);
        assert!(registry.leakage_audit().passed);
    }

    #[test]
    fn v1_forbidden_file_and_symbol_fields_are_supported() {
        let registry = registry();
        let wrong_file = registry
            .tasks
            .iter()
            .find(|task| task.task_id == "local_wrong_file_trap_discount")
            .expect("wrong-file trap");
        assert!(!wrong_file.forbidden_files.is_empty());
        assert!(!wrong_file.forbidden_symbols.is_empty());
        let test_mock = registry
            .tasks
            .iter()
            .find(|task| task.task_id == "local_test_mock_leakage_mailer")
            .expect("test/mock leakage trap");
        assert!(test_mock
            .forbidden_files
            .iter()
            .any(|path| path.contains("tests/helpers")));
    }

    #[test]
    fn v1_local_fixture_checkout_setup_paths_exist() {
        registry()
            .validate_fixture_checkouts(&workspace_root())
            .expect("fixture paths validate");
    }

    #[test]
    fn v1_task_registry_rejects_duplicate_task_ids() {
        let mut registry = registry();
        let duplicate = registry.tasks[0].clone();
        registry.tasks.push(duplicate);
        let error = registry.validate().expect_err("duplicate should fail");
        assert!(error.to_string().contains("duplicate v1 task id"));
    }

    #[test]
    fn v1_pinned_dataset_manifest_validates_materialized_swebench_ladders() {
        let registry = registry();
        let focused = registry
            .pinned_datasets
            .datasets
            .iter()
            .find(|dataset| dataset.dataset_name == "swebench_lite_focused")
            .expect("swebench focused dataset");
        assert_eq!(focused.status, "ready");
        assert!(focused.data_available_in_this_checkout);
        assert_eq!(focused.task_ids, vec!["swebench_lite_sympy_sympy_20590"]);

        let ladders = registry
            .pinned_datasets
            .datasets
            .iter()
            .find(|dataset| dataset.dataset_name == "swebench_lite_ladders_1_5_10")
            .expect("swebench ladder dataset");
        assert_eq!(ladders.status, "ready");
        assert!(ladders.data_available_in_this_checkout);
        assert_eq!(ladders.task_ids.len(), 10);
        assert!(!ladders
            .task_ids
            .iter()
            .any(|task_id| task_id.ends_with("_blocked")));
    }

    #[test]
    fn v1_swebench_ladder_task_ids_are_concrete_and_unique() {
        let registry = registry();
        let ladder_tasks = registry
            .tasks
            .iter()
            .filter(|task| {
                task.dataset_name == "swebench_lite_focused"
                    || task.dataset_name == "swebench_lite_ladders_1_5_10"
            })
            .collect::<Vec<_>>();
        assert_eq!(ladder_tasks.len(), 10);
        let mut seen = BTreeSet::new();
        for task in ladder_tasks {
            assert!(seen.insert(task.task_id.as_str()));
            assert_eq!(task.status, "ready");
            assert!(!task.task_id.ends_with("_blocked"));
            assert!(task
                .repo_source
                .starts_with("princeton-nlp/SWE-bench_Lite:"));
            assert!(task
                .checkout_path_policy
                .contains("validated_local_source_checkout"));
        }
    }

    #[test]
    fn v1_registry_json_schema_rejects_missing_required_fields() {
        let mut raw = serde_json::to_value(registry()).expect("registry value");
        raw["tasks"][0]["task_id"] = json!(null);
        assert!(validate_v1_patch_task_registry_json(&raw).is_err());
    }
}
