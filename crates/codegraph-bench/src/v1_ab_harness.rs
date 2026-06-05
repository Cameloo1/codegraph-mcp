//! Benchmark Layer v1 same-agent A/B harness contract.
//!
//! The v1 product question is not "CodeGraph vs rg"; it is the same coding
//! agent with normal rg/search/edit/test tools in both arms, plus CodeGraph only
//! in the B arm. This module owns the durable run artifact contract and the
//! invariant checker for that comparison. Real patch execution is gated on an
//! explicit external-agent command.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    build_v1_codegraph_context_injection, default_v1_codegraph_context_budget,
    load_v1_patch_task_registry, render_v1_codegraph_context_prompt,
    score_v1_patch_outcome_evidence_value, v1_context_injection_to_value,
    v1_registry::v1_registry_path, BenchResult, BenchmarkError, V1AgentVisibleTask,
    V1CodeGraphContextBudget, V1CodeGraphContextInjection, V1CodeGraphContextScenario,
    V1PatchOutcomeScoringInput, V1PatchTask, BENCH_SCHEMA_VERSION,
};

pub const CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV: &str =
    "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND";
pub const V1_AB_HARNESS_SCHEMA_VERSION: u32 = 1;
pub const V1_AB_HARNESS_NAME: &str = "benchmark_v1_same_agent_ab_harness";
pub const V1_AB_REQUIRED_ARTIFACT_ROOT: &str =
    "reports/audit/artifacts/benchmark_v1_real_agent_patch_outcomes";

pub const REQUIRED_V1_AGENT_MODES: [V1AgentMode; 6] = [
    V1AgentMode::RgOnlyAgentPlan,
    V1AgentMode::RgPlusCodegraphAgentPlan,
    V1AgentMode::RgOnlyAgentPatch,
    V1AgentMode::RgPlusCodegraphAgentPatch,
    V1AgentMode::MockAgentScaffoldOnly,
    V1AgentMode::PlanOnlyDiagnostic,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum V1AgentMode {
    RgOnlyAgentPlan,
    RgPlusCodegraphAgentPlan,
    RgOnlyAgentPatch,
    RgPlusCodegraphAgentPatch,
    MockAgentScaffoldOnly,
    PlanOnlyDiagnostic,
}

impl V1AgentMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RgOnlyAgentPlan => "rg_only_agent_plan",
            Self::RgPlusCodegraphAgentPlan => "rg_plus_codegraph_agent_plan",
            Self::RgOnlyAgentPatch => "rg_only_agent_patch",
            Self::RgPlusCodegraphAgentPatch => "rg_plus_codegraph_agent_patch",
            Self::MockAgentScaffoldOnly => "mock_agent_scaffold_only",
            Self::PlanOnlyDiagnostic => "plan_only_diagnostic",
        }
    }

    pub const fn arm(self) -> &'static str {
        match self {
            Self::RgPlusCodegraphAgentPlan | Self::RgPlusCodegraphAgentPatch => "rg_plus_codegraph",
            Self::RgOnlyAgentPlan | Self::RgOnlyAgentPatch => "rg_only",
            Self::MockAgentScaffoldOnly => "mock_scaffold",
            Self::PlanOnlyDiagnostic => "diagnostic",
        }
    }

    pub const fn is_codegraph_arm(self) -> bool {
        matches!(
            self,
            Self::RgPlusCodegraphAgentPlan | Self::RgPlusCodegraphAgentPatch
        )
    }

    pub const fn is_patch_mode(self) -> bool {
        matches!(
            self,
            Self::RgOnlyAgentPatch | Self::RgPlusCodegraphAgentPatch
        )
    }

    pub const fn is_plan_mode(self) -> bool {
        matches!(
            self,
            Self::RgOnlyAgentPlan | Self::RgPlusCodegraphAgentPlan | Self::PlanOnlyDiagnostic
        )
    }

    pub const fn claim_scope(self) -> &'static str {
        match self {
            Self::MockAgentScaffoldOnly => "scaffold_only",
            Self::PlanOnlyDiagnostic | Self::RgOnlyAgentPlan | Self::RgPlusCodegraphAgentPlan => {
                "plan_only_diagnostic"
            }
            Self::RgOnlyAgentPatch | Self::RgPlusCodegraphAgentPatch => "real_patch_quality_gated",
        }
    }
}

impl std::fmt::Display for V1AgentMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for V1AgentMode {
    type Err = BenchmarkError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().replace('-', "_").as_str() {
            "rg_only_agent_plan" => Ok(Self::RgOnlyAgentPlan),
            "rg_plus_codegraph_agent_plan" => Ok(Self::RgPlusCodegraphAgentPlan),
            "rg_only_agent_patch" => Ok(Self::RgOnlyAgentPatch),
            "rg_plus_codegraph_agent_patch" => Ok(Self::RgPlusCodegraphAgentPatch),
            "mock_agent_scaffold_only" => Ok(Self::MockAgentScaffoldOnly),
            "plan_only_diagnostic" => Ok(Self::PlanOnlyDiagnostic),
            other => Err(BenchmarkError::Validation(format!(
                "unknown Benchmark v1 agent mode {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V1SameAgentAbHarnessOptions {
    pub run_id: Option<String>,
    pub run_root: Option<PathBuf>,
    pub registry_path: Option<PathBuf>,
    pub task_id: Option<String>,
    pub modes: Vec<V1AgentMode>,
    pub external_agent_command: Option<String>,
    pub allow_real_patch_execution: bool,
    pub model: String,
    pub agent_scaffold_version: String,
    pub system_prompt_base: String,
    pub environment_allowlist: Vec<String>,
    pub docker_image: String,
    pub seed: Option<u64>,
    pub codegraph_context_budget: V1CodeGraphContextBudget,
    pub codegraph_context_scenario: V1CodeGraphContextScenario,
}

pub fn default_v1_same_agent_ab_harness_options() -> V1SameAgentAbHarnessOptions {
    V1SameAgentAbHarnessOptions {
        run_id: None,
        run_root: None,
        registry_path: None,
        task_id: None,
        modes: REQUIRED_V1_AGENT_MODES.to_vec(),
        external_agent_command: env::var(CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV).ok(),
        allow_real_patch_execution: false,
        model: "external_agent_configured_model_fixed_by_runner".to_string(),
        agent_scaffold_version: "benchmark_v1_same_agent_scaffold_contract_v1".to_string(),
        system_prompt_base: "Use the same coding-agent scaffold. Use normal rg/search/edit/test tools. Do not expose evaluator-only gold. Preserve proof boundaries and do not make public benchmark claims.".to_string(),
        environment_allowlist: vec![
            CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV.to_string(),
            "PATH".to_string(),
            "PYTHONPATH".to_string(),
            "RUST_LOG".to_string(),
        ],
        docker_image: "task_defined_or_local_process".to_string(),
        seed: Some(1),
        codegraph_context_budget: default_v1_codegraph_context_budget(),
        codegraph_context_scenario: V1CodeGraphContextScenario::FreshCandidateOnly,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1SameAgentAbHarnessArtifacts {
    pub run_id: String,
    pub run_root: String,
    pub run_plan_json: String,
    pub commands_jsonl: String,
    pub blocked_tracks_json: String,
    pub environment_json: String,
    pub artifact_manifest_json: String,
    pub invariant_checks_json: String,
    pub arm_artifacts: Vec<V1ArmArtifacts>,
    pub invariant_checks: Vec<V1SameAgentInvariantCheck>,
    pub patch_modes_status: String,
    pub gold_leakage_audit_passed: bool,
    pub normal_dot_codegraph_mutated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1ArmArtifacts {
    pub task_id: String,
    pub mode: String,
    pub arm: String,
    pub status: String,
    pub claim_scope: String,
    pub artifact_dir: String,
    pub workspace_path: String,
    pub run_plan_json: String,
    pub commands_jsonl: String,
    pub agent_prompt_md: String,
    pub agent_prompt_json: String,
    pub agent_transcript_md: String,
    pub agent_transcript_json: String,
    pub tool_call_log_jsonl: String,
    pub context_injected_json: String,
    pub patch_diff: Option<String>,
    pub test_stdout: String,
    pub test_stderr: String,
    pub score_json: String,
    pub timing_breakdown_json: String,
    pub quality_per_budget_json: String,
    pub blocked_tracks_json: String,
    pub environment_json: String,
    pub artifact_manifest_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V1ArmRunPlan {
    pub schema_version: u32,
    pub harness: String,
    pub generated_before_execution: bool,
    pub run_id: String,
    pub task_id: String,
    pub mode: V1AgentMode,
    pub arm: String,
    pub claim_scope: String,
    pub model: String,
    pub external_agent_command: Vec<String>,
    pub external_agent_env_var: String,
    pub agent_scaffold_version: String,
    pub system_prompt_base: String,
    pub task_prompt_base: String,
    pub repo_source: String,
    pub repo_commit: String,
    pub setup_command: String,
    pub test_command: String,
    pub timeout_ms: u64,
    pub token_budget: u64,
    pub tool_budget: u64,
    pub environment_allowlist: Vec<String>,
    pub docker_image: String,
    pub evaluator_kind: String,
    pub seed: Option<u64>,
    pub visible_task: V1AgentVisibleTask,
    pub workspace_path: String,
    pub artifact_dir: String,
    pub codegraph_available: bool,
    pub codegraph_context_injected: bool,
    pub codegraph_validation_commands_available: bool,
    pub hidden_gold_fields_excluded: bool,
    pub public_claim_allowed: bool,
    pub no_gold_leakage_expected: bool,
    pub real_agent_patch_quality_claim: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1SameAgentInvariantCheck {
    pub passed: bool,
    pub left_mode: String,
    pub right_mode: String,
    pub checked_equal_fields: Vec<String>,
    pub allowed_difference_fields: Vec<String>,
    pub observed_allowed_differences: Vec<String>,
    pub mismatched_forbidden_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1CommandRecord {
    pub schema_version: u32,
    pub run_id: String,
    pub task_id: String,
    pub mode: String,
    pub command_kind: String,
    pub status: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub timeout_ms: u64,
    pub stdout_path: String,
    pub stderr_path: String,
    pub exit_code: Option<i32>,
    pub elapsed_ms: u64,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct ArmExecutionResult {
    status: String,
    claim_scope: String,
    patch_diff: Option<String>,
    transcript_md: String,
    transcript_json: Value,
    tool_calls: Vec<Value>,
    command_records: Vec<V1CommandRecord>,
    score: Value,
    timing: Value,
    quality_per_budget: Value,
    blocked_tracks: Value,
}

pub fn run_v1_same_agent_ab_harness(
    options: V1SameAgentAbHarnessOptions,
) -> BenchResult<V1SameAgentAbHarnessArtifacts> {
    let workspace_root = workspace_root();
    let dot_codegraph_before = dot_codegraph_state(&workspace_root);
    let registry_path = options
        .registry_path
        .clone()
        .unwrap_or_else(|| v1_registry_path(&workspace_root));
    let registry = load_v1_patch_task_registry(&registry_path)?;
    let task = select_task(&registry.tasks, options.task_id.as_deref())?.clone();
    let run_id = options
        .run_id
        .clone()
        .unwrap_or_else(|| format!("v1-same-agent-ab-{}", unix_ms()));
    let run_root = options.run_root.clone().unwrap_or_else(|| {
        PathBuf::from("target")
            .join("codegraph-bench-runs")
            .join(&run_id)
    });
    fs::create_dir_all(&run_root)?;

    let modes = if options.modes.is_empty() {
        REQUIRED_V1_AGENT_MODES.to_vec()
    } else {
        options.modes.clone()
    };
    let external_agent_command = configured_external_agent_command(&options)?;
    let root_run_plan_path = run_root.join("run_plan.json");
    let root_commands_path = run_root.join("commands.jsonl");
    let root_blocked_path = run_root.join("blocked_tracks.json");
    let root_environment_path = run_root.join("environment.json");
    let root_manifest_path = run_root.join("artifact_manifest.json");
    let invariant_checks_path = run_root.join("invariant_checks.json");

    let root_run_plan = json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "bench_schema_version": BENCH_SCHEMA_VERSION,
        "harness": V1_AB_HARNESS_NAME,
        "generated_before_execution": true,
        "run_id": run_id,
        "registry_path": path_string(&registry_path),
        "task_id": task.task_id,
        "modes": modes.iter().map(|mode| mode.as_str()).collect::<Vec<_>>(),
        "same_agent_contract": same_agent_contract_json(),
        "external_agent_env_var": CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV,
        "external_agent_configured": !external_agent_command.is_empty(),
        "allow_real_patch_execution": options.allow_real_patch_execution,
        "public_claim": false,
        "real_agent_patch_quality_claim": false,
    });
    write_json(&root_run_plan_path, &root_run_plan)?;

    let mut arm_artifacts = Vec::new();
    let mut arm_plans = BTreeMap::<V1AgentMode, V1ArmRunPlan>::new();
    let mut root_command_records = Vec::new();
    let mut root_blocked_tracks = Vec::new();

    for mode in modes {
        let arm_dir = run_root
            .join("arms")
            .join(&task.task_id)
            .join(mode.as_str());
        let workspace_path = arm_dir.join("workspace");
        fs::create_dir_all(&arm_dir)?;
        materialize_task_workspace(&workspace_root, &task, &workspace_path)?;

        let arm_plan = build_arm_run_plan(
            &run_id,
            &task,
            mode,
            &workspace_path,
            &arm_dir,
            &external_agent_command,
            &options,
        );
        write_json(&arm_dir.join("run_plan.json"), &arm_plan)?;

        let context_injection = context_injected_json(&arm_plan, &task, &options)?;
        let context_value = v1_context_injection_to_value(&context_injection)?;
        write_json(&arm_dir.join("context_injected.json"), &context_value)?;

        let prompt_md = render_agent_prompt(&arm_plan, &context_injection)?;
        let mut prompt_json = json!({
            "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
            "task": arm_plan.visible_task,
            "system_prompt_base": arm_plan.system_prompt_base,
            "task_prompt_base": arm_plan.task_prompt_base,
            "mode": arm_plan.mode.as_str(),
            "arm": arm_plan.arm,
            "codegraph_context_injected": arm_plan.codegraph_context_injected,
            "hidden_gold_fields_excluded": true,
            "public_claim_allowed": false,
        });
        if let Some(packet) = context_injection.packet.as_ref() {
            prompt_json["codegraph_context_packet"] = serde_json::to_value(packet)
                .map_err(|error| BenchmarkError::Parse(error.to_string()))?;
            prompt_json["codegraph_attribution"] =
                serde_json::to_value(&context_injection.attribution)
                    .map_err(|error| BenchmarkError::Parse(error.to_string()))?;
        }
        write_text(&arm_dir.join("agent_prompt.md"), &prompt_md)?;
        write_json(&arm_dir.join("agent_prompt.json"), &prompt_json)?;
        validate_agent_prompt_no_gold_leakage(&task, &prompt_md, &prompt_json)?;

        let execution = execute_mode(
            &arm_plan,
            &task,
            &arm_dir,
            &external_agent_command,
            options.allow_real_patch_execution,
            &context_injection,
        )?;

        write_text(
            &arm_dir.join("agent_transcript.md"),
            &execution.transcript_md,
        )?;
        write_json(
            &arm_dir.join("agent_transcript.json"),
            &execution.transcript_json,
        )?;
        write_jsonl(&arm_dir.join("tool_call_log.jsonl"), &execution.tool_calls)?;
        write_jsonl(
            &arm_dir.join("commands.jsonl"),
            &command_records_to_values(&execution.command_records)?,
        )?;
        let stdout_path = arm_dir.join("test_output.stdout.txt");
        let stderr_path = arm_dir.join("test_output.stderr.txt");
        if !stdout_path.is_file() {
            write_text(&stdout_path, "")?;
        }
        if !stderr_path.is_file() {
            write_text(&stderr_path, "")?;
        }
        write_json(&arm_dir.join("score.json"), &execution.score)?;
        write_json(&arm_dir.join("timing_breakdown.json"), &execution.timing)?;
        write_json(
            &arm_dir.join("quality_per_budget.json"),
            &execution.quality_per_budget,
        )?;
        write_json(
            &arm_dir.join("blocked_tracks.json"),
            &execution.blocked_tracks,
        )?;
        write_json(
            &arm_dir.join("environment.json"),
            &environment_json(&options),
        )?;
        let patch_path = if let Some(diff) = execution.patch_diff.as_deref() {
            let path = arm_dir.join("patch.diff");
            write_text(&path, diff)?;
            Some(path_string(&path))
        } else {
            None
        };

        let artifacts = V1ArmArtifacts {
            task_id: task.task_id.clone(),
            mode: mode.as_str().to_string(),
            arm: mode.arm().to_string(),
            status: execution.status.clone(),
            claim_scope: execution.claim_scope.clone(),
            artifact_dir: path_string(&arm_dir),
            workspace_path: path_string(&workspace_path),
            run_plan_json: path_string(&arm_dir.join("run_plan.json")),
            commands_jsonl: path_string(&arm_dir.join("commands.jsonl")),
            agent_prompt_md: path_string(&arm_dir.join("agent_prompt.md")),
            agent_prompt_json: path_string(&arm_dir.join("agent_prompt.json")),
            agent_transcript_md: path_string(&arm_dir.join("agent_transcript.md")),
            agent_transcript_json: path_string(&arm_dir.join("agent_transcript.json")),
            tool_call_log_jsonl: path_string(&arm_dir.join("tool_call_log.jsonl")),
            context_injected_json: path_string(&arm_dir.join("context_injected.json")),
            patch_diff: patch_path,
            test_stdout: path_string(&arm_dir.join("test_output.stdout.txt")),
            test_stderr: path_string(&arm_dir.join("test_output.stderr.txt")),
            score_json: path_string(&arm_dir.join("score.json")),
            timing_breakdown_json: path_string(&arm_dir.join("timing_breakdown.json")),
            quality_per_budget_json: path_string(&arm_dir.join("quality_per_budget.json")),
            blocked_tracks_json: path_string(&arm_dir.join("blocked_tracks.json")),
            environment_json: path_string(&arm_dir.join("environment.json")),
            artifact_manifest_json: path_string(&arm_dir.join("artifact_manifest.json")),
        };
        write_json(
            &arm_dir.join("artifact_manifest.json"),
            &arm_manifest(&artifacts),
        )?;

        root_command_records.extend(execution.command_records);
        if execution.blocked_tracks["blocked"]
            .as_bool()
            .unwrap_or(false)
        {
            root_blocked_tracks.push(json!({
                "task_id": task.task_id,
                "mode": mode.as_str(),
                "blocked_tracks": execution.blocked_tracks,
            }));
        }
        arm_plans.insert(mode, arm_plan);
        arm_artifacts.push(artifacts);
    }

    let mut invariant_checks = Vec::new();
    if let (Some(left), Some(right)) = (
        arm_plans.get(&V1AgentMode::RgOnlyAgentPlan),
        arm_plans.get(&V1AgentMode::RgPlusCodegraphAgentPlan),
    ) {
        invariant_checks.push(assert_v1_same_agent_invariants(left, right)?);
    }
    if let (Some(left), Some(right)) = (
        arm_plans.get(&V1AgentMode::RgOnlyAgentPatch),
        arm_plans.get(&V1AgentMode::RgPlusCodegraphAgentPatch),
    ) {
        invariant_checks.push(assert_v1_same_agent_invariants(left, right)?);
    }

    write_jsonl(
        &root_commands_path,
        &command_records_to_values(&root_command_records)?,
    )?;
    write_json(&root_blocked_path, &root_blocked_tracks)?;
    write_json(&root_environment_path, &environment_json(&options))?;
    write_json(&invariant_checks_path, &invariant_checks)?;

    let patch_modes_status = patch_modes_status(&arm_artifacts);
    let dot_codegraph_after = dot_codegraph_state(&workspace_root);
    let normal_dot_codegraph_mutated = dot_codegraph_before != dot_codegraph_after;
    let artifacts = V1SameAgentAbHarnessArtifacts {
        run_id: run_id.clone(),
        run_root: path_string(&run_root),
        run_plan_json: path_string(&root_run_plan_path),
        commands_jsonl: path_string(&root_commands_path),
        blocked_tracks_json: path_string(&root_blocked_path),
        environment_json: path_string(&root_environment_path),
        artifact_manifest_json: path_string(&root_manifest_path),
        invariant_checks_json: path_string(&invariant_checks_path),
        arm_artifacts,
        invariant_checks,
        patch_modes_status,
        gold_leakage_audit_passed: true,
        normal_dot_codegraph_mutated,
    };
    write_json(&root_manifest_path, &root_manifest(&artifacts))?;
    validate_v1_same_agent_ab_artifacts(&root_manifest_path)?;
    Ok(artifacts)
}

pub fn check_v1_same_agent_invariants(
    left: &V1ArmRunPlan,
    right: &V1ArmRunPlan,
) -> V1SameAgentInvariantCheck {
    let checked_equal_fields = [
        "model",
        "external_agent_command",
        "agent_scaffold_version",
        "system_prompt_base",
        "task_prompt_base",
        "repo_commit",
        "setup_command",
        "test_command",
        "timeout_ms",
        "token_budget",
        "tool_budget",
        "environment_allowlist",
        "docker_image",
        "evaluator_kind",
        "seed",
        "task_id",
        "repo_source",
    ]
    .iter()
    .map(|value| (*value).to_string())
    .collect::<Vec<_>>();
    let allowed_difference_fields = [
        "mode",
        "arm",
        "artifact_dir",
        "workspace_path",
        "codegraph_available",
        "codegraph_context_injected",
        "codegraph_validation_commands_available",
    ]
    .iter()
    .map(|value| (*value).to_string())
    .collect::<Vec<_>>();
    let mut mismatched_forbidden_fields = Vec::new();
    let mut observed_allowed_differences = Vec::new();

    macro_rules! check_equal {
        ($field:ident) => {
            if left.$field != right.$field {
                mismatched_forbidden_fields.push(stringify!($field).to_string());
            }
        };
    }

    check_equal!(model);
    check_equal!(external_agent_command);
    check_equal!(agent_scaffold_version);
    check_equal!(system_prompt_base);
    check_equal!(task_prompt_base);
    check_equal!(repo_source);
    check_equal!(repo_commit);
    check_equal!(setup_command);
    check_equal!(test_command);
    check_equal!(timeout_ms);
    check_equal!(token_budget);
    check_equal!(tool_budget);
    check_equal!(environment_allowlist);
    check_equal!(docker_image);
    check_equal!(evaluator_kind);
    check_equal!(seed);
    check_equal!(task_id);

    if left.mode != right.mode {
        observed_allowed_differences.push("mode".to_string());
    }
    if left.arm != right.arm {
        observed_allowed_differences.push("arm".to_string());
    }
    if left.artifact_dir != right.artifact_dir {
        observed_allowed_differences.push("artifact_dir".to_string());
    }
    if left.workspace_path != right.workspace_path {
        observed_allowed_differences.push("workspace_path".to_string());
    }
    if left.codegraph_available != right.codegraph_available {
        observed_allowed_differences.push("codegraph_available".to_string());
    }
    if left.codegraph_context_injected != right.codegraph_context_injected {
        observed_allowed_differences.push("codegraph_context_injected".to_string());
    }
    if left.codegraph_validation_commands_available != right.codegraph_validation_commands_available
    {
        observed_allowed_differences.push("codegraph_validation_commands_available".to_string());
    }

    V1SameAgentInvariantCheck {
        passed: mismatched_forbidden_fields.is_empty(),
        left_mode: left.mode.as_str().to_string(),
        right_mode: right.mode.as_str().to_string(),
        checked_equal_fields,
        allowed_difference_fields,
        observed_allowed_differences,
        mismatched_forbidden_fields,
    }
}

pub fn assert_v1_same_agent_invariants(
    left: &V1ArmRunPlan,
    right: &V1ArmRunPlan,
) -> BenchResult<V1SameAgentInvariantCheck> {
    let check = check_v1_same_agent_invariants(left, right);
    if check.passed {
        Ok(check)
    } else {
        Err(BenchmarkError::Validation(format!(
            "Benchmark v1 same-agent invariant failed for {} vs {}: {:?}",
            check.left_mode, check.right_mode, check.mismatched_forbidden_fields
        )))
    }
}

pub fn validate_v1_same_agent_ab_artifacts(manifest_path: &Path) -> BenchResult<()> {
    let raw = fs::read_to_string(manifest_path)?;
    let manifest: Value =
        serde_json::from_str(&raw).map_err(|error| BenchmarkError::Parse(error.to_string()))?;
    if manifest["schema_version"].as_u64() != Some(V1_AB_HARNESS_SCHEMA_VERSION as u64) {
        return Err(BenchmarkError::Validation(format!(
            "v1 A/B artifact manifest {} has wrong schema version",
            manifest_path.display()
        )));
    }
    let required = manifest["required_artifacts"]
        .as_array()
        .ok_or_else(|| BenchmarkError::Validation("manifest missing required_artifacts".into()))?;
    for artifact in required {
        let path = artifact["path"].as_str().ok_or_else(|| {
            BenchmarkError::Validation("manifest required artifact missing path".into())
        })?;
        if !Path::new(path).exists() {
            return Err(BenchmarkError::Validation(format!(
                "required v1 A/B artifact missing: {path}"
            )));
        }
    }
    for path in manifest["json_artifacts"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(Value::as_str)
    {
        let value: Value = serde_json::from_str(&fs::read_to_string(path)?)
            .map_err(|error| BenchmarkError::Parse(format!("{path}: {error}")))?;
        if value.is_null() {
            return Err(BenchmarkError::Validation(format!(
                "JSON artifact unexpectedly null: {path}"
            )));
        }
    }
    for path in manifest["jsonl_artifacts"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(Value::as_str)
    {
        validate_jsonl(path)?;
    }
    Ok(())
}

fn select_task<'a>(
    tasks: &'a [V1PatchTask],
    task_id: Option<&str>,
) -> BenchResult<&'a V1PatchTask> {
    if let Some(task_id) = task_id {
        return tasks
            .iter()
            .find(|task| task.task_id == task_id)
            .ok_or_else(|| BenchmarkError::Validation(format!("unknown v1 task id {task_id}")));
    }
    tasks
        .iter()
        .find(|task| task.repo_source == "local_fixture" && task.status == "ready")
        .ok_or_else(|| {
            BenchmarkError::Validation(
                "v1 same-agent harness needs at least one ready local fixture task".to_string(),
            )
        })
}

fn configured_external_agent_command(
    options: &V1SameAgentAbHarnessOptions,
) -> BenchResult<Vec<String>> {
    let raw = options
        .external_agent_command
        .clone()
        .or_else(|| env::var(CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV).ok());
    match raw {
        Some(raw) if !raw.trim().is_empty() => parse_external_agent_command(&raw),
        _ => Ok(Vec::new()),
    }
}

fn parse_external_agent_command(raw: &str) -> BenchResult<Vec<String>> {
    if raw.trim_start().starts_with('[') {
        let parsed = serde_json::from_str::<Vec<String>>(raw)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?;
        if parsed.is_empty() || parsed.iter().any(|part| part.trim().is_empty()) {
            return Err(BenchmarkError::Validation(
                "external-agent command JSON array must contain non-empty argv entries".to_string(),
            ));
        }
        return Ok(parsed);
    }
    let parsed = split_command_line(raw)?;
    if parsed.is_empty() {
        return Err(BenchmarkError::Validation(
            "external-agent command must not be empty".to_string(),
        ));
    }
    Ok(parsed)
}

fn split_command_line(raw: &str) -> BenchResult<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for character in raw.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            value if value.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if escaped || in_quotes {
        return Err(BenchmarkError::Parse(
            "external-agent command has unterminated quote or escape".to_string(),
        ));
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn build_arm_run_plan(
    run_id: &str,
    task: &V1PatchTask,
    mode: V1AgentMode,
    workspace_path: &Path,
    artifact_dir: &Path,
    external_agent_command: &[String],
    options: &V1SameAgentAbHarnessOptions,
) -> V1ArmRunPlan {
    V1ArmRunPlan {
        schema_version: V1_AB_HARNESS_SCHEMA_VERSION,
        harness: V1_AB_HARNESS_NAME.to_string(),
        generated_before_execution: true,
        run_id: run_id.to_string(),
        task_id: task.task_id.clone(),
        mode,
        arm: mode.arm().to_string(),
        claim_scope: mode.claim_scope().to_string(),
        model: options.model.clone(),
        external_agent_command: external_agent_command.to_vec(),
        external_agent_env_var: CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV.to_string(),
        agent_scaffold_version: options.agent_scaffold_version.clone(),
        system_prompt_base: options.system_prompt_base.clone(),
        task_prompt_base: task.task_prompt.clone(),
        repo_source: task.repo_source.clone(),
        repo_commit: task.repo_commit.clone(),
        setup_command: task.setup_command.clone(),
        test_command: task.test_command.clone(),
        timeout_ms: task.timeout_ms,
        token_budget: task.token_budget,
        tool_budget: task.tool_budget,
        environment_allowlist: options.environment_allowlist.clone(),
        docker_image: if options.docker_image == "task_defined_or_local_process" {
            task.docker_image_or_env.clone()
        } else {
            options.docker_image.clone()
        },
        evaluator_kind: task.evaluator_kind.clone(),
        seed: options.seed,
        visible_task: task.agent_visible(),
        workspace_path: path_string(workspace_path),
        artifact_dir: path_string(artifact_dir),
        codegraph_available: mode.is_codegraph_arm(),
        codegraph_context_injected: mode.is_codegraph_arm(),
        codegraph_validation_commands_available: mode.is_codegraph_arm(),
        hidden_gold_fields_excluded: true,
        public_claim_allowed: false,
        no_gold_leakage_expected: true,
        real_agent_patch_quality_claim: false,
    }
}

fn execute_mode(
    arm_plan: &V1ArmRunPlan,
    task: &V1PatchTask,
    arm_dir: &Path,
    external_agent_command: &[String],
    allow_real_patch_execution: bool,
    context_injection: &V1CodeGraphContextInjection,
) -> BenchResult<ArmExecutionResult> {
    let started = Instant::now();
    if arm_plan.mode.is_patch_mode() && external_agent_command.is_empty() {
        let record = blocked_command_record(
            arm_plan,
            arm_dir,
            "external_agent",
            "blocked_external_agent_missing",
            format!("{CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV} is unset"),
            Vec::new(),
            started.elapsed().as_millis() as u64,
        );
        return Ok(blocked_result(
            arm_plan,
            task,
            started,
            "blocked_external_agent_missing",
            format!(
                "Set {CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV} to a validated current external-agent command before real patch modes can run."
            ),
            vec![record],
            context_injection,
        ));
    }

    if arm_plan.mode.is_patch_mode() && !allow_real_patch_execution {
        let argv = external_agent_argv(arm_plan, arm_dir, external_agent_command);
        let record = blocked_command_record(
            arm_plan,
            arm_dir,
            "external_agent",
            "ready_for_real_patch_run",
            "real patch execution was not enabled for this scaffold prompt".to_string(),
            argv,
            started.elapsed().as_millis() as u64,
        );
        return Ok(blocked_result(
            arm_plan,
            task,
            started,
            "ready_for_real_patch_run",
            "External-agent command is configured, but this harness invocation did not enable real patch execution.".to_string(),
            vec![record],
            context_injection,
        ));
    }

    if arm_plan.mode.is_patch_mode() {
        return execute_external_agent(
            arm_plan,
            task,
            arm_dir,
            external_agent_command,
            started,
            context_injection,
        );
    }

    if arm_plan.mode == V1AgentMode::MockAgentScaffoldOnly {
        let patch = scaffold_patch(task);
        let command_record = scaffold_command_record(
            arm_plan,
            arm_dir,
            "mock_agent_scaffold_only",
            vec![
                "mock_agent_scaffold_only".to_string(),
                "--task-json".to_string(),
                path_string(&arm_dir.join("agent_prompt.json")),
                "--workspace".to_string(),
                arm_plan.workspace_path.clone(),
                "--output-dir".to_string(),
                path_string(arm_dir),
            ],
            started.elapsed().as_millis() as u64,
        );
        return Ok(scaffold_result(
            arm_plan,
            task,
            started,
            "scaffold_only",
            Some(patch),
            vec![command_record],
            context_injection,
        ));
    }

    let command_record = scaffold_command_record(
        arm_plan,
        arm_dir,
        "plan_only_diagnostic",
        vec![
            "plan_only_diagnostic".to_string(),
            "--task-json".to_string(),
            path_string(&arm_dir.join("agent_prompt.json")),
            "--workspace".to_string(),
            arm_plan.workspace_path.clone(),
            "--output-dir".to_string(),
            path_string(arm_dir),
        ],
        started.elapsed().as_millis() as u64,
    );
    Ok(scaffold_result(
        arm_plan,
        task,
        started,
        "plan_only_diagnostic",
        None,
        vec![command_record],
        context_injection,
    ))
}

fn execute_external_agent(
    arm_plan: &V1ArmRunPlan,
    task: &V1PatchTask,
    arm_dir: &Path,
    external_agent_command: &[String],
    started: Instant,
    context_injection: &V1CodeGraphContextInjection,
) -> BenchResult<ArmExecutionResult> {
    let argv = external_agent_argv(arm_plan, arm_dir, external_agent_command);
    let stdout_path = arm_dir.join("test_output.stdout.txt");
    let stderr_path = arm_dir.join("test_output.stderr.txt");
    let command_started = Instant::now();
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(&arm_plan.workspace_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let timeout = Duration::from_millis(arm_plan.timeout_ms);
    let timed_out = loop {
        if child.try_wait()?.is_some() {
            break false;
        }
        if command_started.elapsed() >= timeout {
            let _ = child.kill();
            break true;
        }
        thread::sleep(Duration::from_millis(25));
    };
    let output = child.wait_with_output()?;
    write_text(&stdout_path, &String::from_utf8_lossy(&output.stdout))?;
    write_text(&stderr_path, &String::from_utf8_lossy(&output.stderr))?;
    let status = if timed_out {
        "timed_out"
    } else if output.status.success() {
        "completed_real_agent_run"
    } else {
        "failed"
    };
    let record = V1CommandRecord {
        schema_version: V1_AB_HARNESS_SCHEMA_VERSION,
        run_id: arm_plan.run_id.clone(),
        task_id: arm_plan.task_id.clone(),
        mode: arm_plan.mode.as_str().to_string(),
        command_kind: "external_agent".to_string(),
        status: status.to_string(),
        argv,
        cwd: arm_plan.workspace_path.clone(),
        timeout_ms: arm_plan.timeout_ms,
        stdout_path: path_string(&stdout_path),
        stderr_path: path_string(&stderr_path),
        exit_code: output.status.code(),
        elapsed_ms: command_started.elapsed().as_millis() as u64,
        blocked_reason: None,
    };
    let patch_path = arm_dir.join("patch.diff");
    let patch_diff = patch_path
        .is_file()
        .then(|| fs::read_to_string(&patch_path))
        .transpose()?;
    let transcript_md = format!(
        "# Agent Transcript\n\nExternal agent exited with status `{status}`. Patch quality remains unclaimed until evaluator scoring completes.\n"
    );
    let transcript_json = json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "status": status,
        "claim_scope": "real_patch_quality_gated",
        "real_agent_patch_quality_claim": false,
    });
    let tool_calls = vec![json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "tool": "external_agent",
        "status": status,
        "argv_recorded_exactly": true,
    })];
    let command_records = vec![record];
    let score = score_artifact_json(ScoreArtifactInput {
        task,
        arm_plan,
        status,
        claim_scope: "real_patch_quality_gated",
        patch_diff: patch_diff.as_deref(),
        transcript: &transcript_md,
        tool_calls: &tool_calls,
        command_records: &command_records,
        context_injection,
        started,
    })?;
    Ok(ArmExecutionResult {
        status: status.to_string(),
        claim_scope: "real_patch_quality_gated".to_string(),
        patch_diff,
        transcript_md,
        transcript_json,
        tool_calls,
        command_records,
        score,
        timing: timing_json(started),
        quality_per_budget: quality_per_budget_json(arm_plan, status),
        blocked_tracks: json!({
            "blocked": false,
            "status": status,
            "real_agent_patch_quality_claim": false,
        }),
    })
}

fn blocked_result(
    arm_plan: &V1ArmRunPlan,
    task: &V1PatchTask,
    started: Instant,
    status: &str,
    reason: String,
    command_records: Vec<V1CommandRecord>,
    context_injection: &V1CodeGraphContextInjection,
) -> ArmExecutionResult {
    let transcript_md = format!(
        "# Agent Transcript\n\nRun status: `{status}`.\n\n{reason}\n\nNo patch-quality result was produced.\n"
    );
    let transcript_json = json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "status": status,
        "blocked_reason": reason,
        "real_agent_patch_quality_claim": false,
    });
    let tool_calls = vec![json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "tool": "external_agent",
        "status": status,
        "blocked_reason": reason,
    })];
    let score = score_artifact_json(ScoreArtifactInput {
        task,
        arm_plan,
        status,
        claim_scope: "blocked",
        patch_diff: None,
        transcript: &transcript_md,
        tool_calls: &tool_calls,
        command_records: &command_records,
        context_injection,
        started,
    })
    .unwrap_or_else(|error| {
        json!({
            "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
            "status": status,
            "scoring_error": error.to_string(),
            "public_claim": false,
            "real_agent_patch_quality_claim": false,
        })
    });
    ArmExecutionResult {
        status: status.to_string(),
        claim_scope: "blocked".to_string(),
        patch_diff: None,
        transcript_md,
        transcript_json,
        tool_calls,
        command_records,
        score,
        timing: timing_json(started),
        quality_per_budget: quality_per_budget_json(arm_plan, status),
        blocked_tracks: json!({
            "blocked": status.starts_with("blocked_"),
            "status": status,
            "blocked_reason": reason,
            "env_var_needed": CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV,
            "repair_action": format!("Set {CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV} to a validated current external-agent command and rerun the v1 harness."),
            "real_agent_patch_quality_claim": false,
        }),
    }
}

fn scaffold_result(
    arm_plan: &V1ArmRunPlan,
    task: &V1PatchTask,
    started: Instant,
    status: &str,
    patch_diff: Option<String>,
    command_records: Vec<V1CommandRecord>,
    context_injection: &V1CodeGraphContextInjection,
) -> ArmExecutionResult {
    let plan = scaffold_plan(arm_plan, context_injection);
    let transcript_md = format!(
        "# Agent Transcript\n\nStatus: `{status}`.\n\nPlan:\n\n{plan}\n\nThis is not counted as real agent patch quality.\n"
    );
    let transcript_json = json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "status": status,
        "claim_scope": status,
        "plan": plan,
        "mock_or_plan_counted_as_real_patch_quality": false,
        "real_agent_patch_quality_claim": false,
    });
    let tool_calls = vec![
        json!({
            "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
            "tool": "rg",
            "status": "scaffold_recorded_not_executed",
            "available_in_arm": true,
        }),
        json!({
            "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
            "tool": "codegraph",
            "status": if arm_plan.codegraph_available { "scaffold_context_available_b_arm_only" } else { "not_available_a_arm" },
            "available_in_arm": arm_plan.codegraph_available,
            "graph_proof": false,
        }),
    ];
    let score = score_artifact_json(ScoreArtifactInput {
        task,
        arm_plan,
        status,
        claim_scope: status,
        patch_diff: patch_diff.as_deref(),
        transcript: &transcript_md,
        tool_calls: &tool_calls,
        command_records: &command_records,
        context_injection,
        started,
    })
    .unwrap_or_else(|error| {
        json!({
            "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
            "status": status,
            "scoring_error": error.to_string(),
            "public_claim": false,
            "real_agent_patch_quality_claim": false,
        })
    });
    ArmExecutionResult {
        status: status.to_string(),
        claim_scope: status.to_string(),
        patch_diff,
        transcript_md,
        transcript_json,
        tool_calls,
        command_records,
        score,
        timing: timing_json(started),
        quality_per_budget: quality_per_budget_json(arm_plan, status),
        blocked_tracks: json!({
            "blocked": false,
            "status": status,
            "mock_or_plan_counted_as_real_patch_quality": false,
            "real_agent_patch_quality_claim": false,
        }),
    }
}

fn scaffold_plan(
    arm_plan: &V1ArmRunPlan,
    context_injection: &V1CodeGraphContextInjection,
) -> String {
    let visible_query = arm_plan
        .visible_task
        .visible_query_terms
        .first()
        .map(String::as_str)
        .unwrap_or("visible task terms");
    let visible_surface = arm_plan
        .visible_task
        .visible_file_hints
        .first()
        .map(String::as_str)
        .unwrap_or("visible task surface");
    let mut plan = format!(
        "Use visible task fields only. Search with rg for `{visible_query}`, inspect `{visible_surface}`, run `{}`, and apply a disposable-workspace patch only if this is a real patch run. Mode: {}.",
        arm_plan.test_command,
        arm_plan.mode.as_str()
    );
    if let Some(packet) = context_injection.packet.as_ref() {
        let label = packet
            .proof_paths
            .first()
            .map(|path| path.path_id.as_str())
            .or_else(|| {
                packet
                    .source_navigation_evidence
                    .first()
                    .map(|item| item.label.as_str())
            })
            .or_else(|| packet.text_evidence.first().map(|item| item.label.as_str()))
            .unwrap_or("codegraph_context_packet");
        plan.push_str(&format!(
            " B-arm CodeGraph packet `{label}` is available with status `{}` and {} bytes. Treat candidate, text, and source-navigation evidence as not graph proof.",
            packet.codegraph_context_status,
            packet.context_packet_bytes
        ));
        if packet.codegraph_context_status == "no_proof_path_found" {
            plan.push_str(" Respect `no_proof_path_found` and keep proof status unknown unless graph/source verification exists.");
        }
        if packet.stale_evidence_present {
            plan.push_str(
                " The packet is stale/non-claimable, so CodeGraph attribution must remain invalid.",
            );
        }
    } else {
        plan.push_str(" No CodeGraph packet is available in this arm; keep graph-proof and CodeGraph attribution out of the plan.");
    }
    plan
}

fn external_agent_argv(
    arm_plan: &V1ArmRunPlan,
    arm_dir: &Path,
    external_agent_command: &[String],
) -> Vec<String> {
    let replacements = BTreeMap::from([
        (
            "{task_json}",
            path_string(&arm_dir.join("agent_prompt.json")),
        ),
        ("{workspace}", arm_plan.workspace_path.clone()),
        ("{output_dir}", path_string(arm_dir)),
        ("{mode}", arm_plan.mode.as_str().to_string()),
        ("{run_plan}", path_string(&arm_dir.join("run_plan.json"))),
    ]);
    let mut used_placeholder = false;
    let mut argv = external_agent_command
        .iter()
        .map(|part| {
            let mut replaced = part.clone();
            for (placeholder, value) in &replacements {
                if replaced.contains(placeholder) {
                    used_placeholder = true;
                    replaced = replaced.replace(placeholder, value);
                }
            }
            replaced
        })
        .collect::<Vec<_>>();
    if !used_placeholder {
        argv.extend([
            "--task-json".to_string(),
            path_string(&arm_dir.join("agent_prompt.json")),
            "--workspace".to_string(),
            arm_plan.workspace_path.clone(),
            "--output-dir".to_string(),
            path_string(arm_dir),
            "--mode".to_string(),
            arm_plan.mode.as_str().to_string(),
        ]);
    }
    argv
}

fn scaffold_command_record(
    arm_plan: &V1ArmRunPlan,
    arm_dir: &Path,
    command_kind: &str,
    argv: Vec<String>,
    elapsed_ms: u64,
) -> V1CommandRecord {
    V1CommandRecord {
        schema_version: V1_AB_HARNESS_SCHEMA_VERSION,
        run_id: arm_plan.run_id.clone(),
        task_id: arm_plan.task_id.clone(),
        mode: arm_plan.mode.as_str().to_string(),
        command_kind: command_kind.to_string(),
        status: "scaffold_only".to_string(),
        argv,
        cwd: arm_plan.workspace_path.clone(),
        timeout_ms: arm_plan.timeout_ms,
        stdout_path: path_string(&arm_dir.join("test_output.stdout.txt")),
        stderr_path: path_string(&arm_dir.join("test_output.stderr.txt")),
        exit_code: Some(0),
        elapsed_ms,
        blocked_reason: None,
    }
}

fn blocked_command_record(
    arm_plan: &V1ArmRunPlan,
    arm_dir: &Path,
    command_kind: &str,
    status: &str,
    blocked_reason: String,
    argv: Vec<String>,
    elapsed_ms: u64,
) -> V1CommandRecord {
    V1CommandRecord {
        schema_version: V1_AB_HARNESS_SCHEMA_VERSION,
        run_id: arm_plan.run_id.clone(),
        task_id: arm_plan.task_id.clone(),
        mode: arm_plan.mode.as_str().to_string(),
        command_kind: command_kind.to_string(),
        status: status.to_string(),
        argv,
        cwd: arm_plan.workspace_path.clone(),
        timeout_ms: arm_plan.timeout_ms,
        stdout_path: path_string(&arm_dir.join("test_output.stdout.txt")),
        stderr_path: path_string(&arm_dir.join("test_output.stderr.txt")),
        exit_code: None,
        elapsed_ms,
        blocked_reason: Some(blocked_reason),
    }
}

fn context_injected_json(
    arm_plan: &V1ArmRunPlan,
    task: &V1PatchTask,
    options: &V1SameAgentAbHarnessOptions,
) -> BenchResult<V1CodeGraphContextInjection> {
    build_v1_codegraph_context_injection(
        task,
        arm_plan.codegraph_context_injected,
        options.codegraph_context_scenario,
        options.codegraph_context_budget,
    )
}

fn render_agent_prompt(
    arm_plan: &V1ArmRunPlan,
    context: &V1CodeGraphContextInjection,
) -> BenchResult<String> {
    let task = &arm_plan.visible_task;
    let mut prompt = format!(
        "# Benchmark v1 Agent Task\n\nMode: `{}`\nArm: `{}`\nTask ID: `{}`\nDataset: `{}`\nRepo source: `{}`\nRepo commit: `{}`\n\n## System Prompt Base\n\n{}\n\n## Task Prompt Base\n\n{}\n\n## Issue Text\n\n{}\n\n## Visible Query Terms\n\n{}\n\n## Visible File Hints\n\n{}\n\n## Visible Symbol Hints\n\n{}\n\n## Visible Error Messages\n\n{}\n\n## Commands\n\nSetup: `{}`\nTest: `{}`\n\n## Budgets\n\nTimeout ms: `{}`\nToken budget: `{}`\nTool budget: `{}`\n\n## Context Boundary\n\nA and B keep the same normal rg/search/edit/test tools. Public benchmark claims are not allowed. Do not infer CodeGraph attribution from patch success.\n",
        arm_plan.mode.as_str(),
        arm_plan.arm,
        task.task_id,
        task.dataset_name,
        task.repo_source,
        task.repo_commit,
        arm_plan.system_prompt_base,
        task.task_prompt,
        task.issue_text,
        render_list(&task.visible_query_terms),
        render_list(&task.visible_file_hints),
        render_list(&task.visible_symbol_hints),
        render_list(&task.visible_error_messages),
        task.setup_command,
        task.test_command,
        task.timeout_ms,
        task.token_budget,
        task.tool_budget
    );
    if let Some(packet) = context.packet.as_ref() {
        prompt.push('\n');
        prompt.push_str(&render_v1_codegraph_context_prompt(packet)?);
    }
    Ok(prompt)
}

fn validate_agent_prompt_no_gold_leakage(
    task: &V1PatchTask,
    prompt_md: &str,
    prompt_json: &Value,
) -> BenchResult<()> {
    let prompt_blob = format!(
        "{prompt_md}\n{}",
        serde_json::to_string(prompt_json)
            .map_err(|error| BenchmarkError::Parse(error.to_string()))?
    );
    let visible_prompt = format!("{}\n{}", task.issue_text, task.task_prompt).to_ascii_lowercase();
    for hidden_value in task
        .hidden_gold_files
        .iter()
        .chain(task.hidden_gold_symbols.iter())
        .chain(task.expected_touched_files.iter())
        .chain(task.expected_tests.iter())
        .filter(|value| !value.trim().is_empty())
    {
        if contains_case_insensitive(&prompt_blob, hidden_value)
            && !visible_prompt.contains(&hidden_value.to_ascii_lowercase())
        {
            return Err(BenchmarkError::Validation(format!(
                "agent prompt leaked evaluator-only field for task {}: {}",
                task.task_id, hidden_value
            )));
        }
    }
    Ok(())
}

fn materialize_task_workspace(
    workspace_root: &Path,
    task: &V1PatchTask,
    destination: &Path,
) -> BenchResult<()> {
    fs::create_dir_all(destination)?;
    if task.repo_source == "local_fixture" {
        let fixture = task.fixture_repo_path.as_deref().ok_or_else(|| {
            BenchmarkError::Validation(format!(
                "local fixture task {} is missing fixture_repo_path",
                task.task_id
            ))
        })?;
        copy_dir_recursive(&workspace_root.join(fixture), destination)?;
    } else {
        write_text(
            &destination.join("README.benchmark_v1_blocked.md"),
            "External dataset checkout is not materialized in this local scaffold run.\n",
        )?;
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> BenchResult<()> {
    if !source.is_dir() {
        return Err(BenchmarkError::Validation(format!(
            "fixture source directory missing: {}",
            source.display()
        )));
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn scaffold_patch(task: &V1PatchTask) -> String {
    let file = task
        .visible_file_hints
        .first()
        .cloned()
        .unwrap_or_else(|| "visible_fixture_surface".to_string());
    format!(
        "diff --git a/{file} b/{file}\n+// mock_agent_scaffold_only: no real repository mutation or patch-quality claim\n"
    )
}

struct ScoreArtifactInput<'a> {
    task: &'a V1PatchTask,
    arm_plan: &'a V1ArmRunPlan,
    status: &'a str,
    claim_scope: &'a str,
    patch_diff: Option<&'a str>,
    transcript: &'a str,
    tool_calls: &'a [Value],
    command_records: &'a [V1CommandRecord],
    context_injection: &'a V1CodeGraphContextInjection,
    started: Instant,
}

fn score_artifact_json(input: ScoreArtifactInput<'_>) -> BenchResult<Value> {
    score_v1_patch_outcome_evidence_value(V1PatchOutcomeScoringInput {
        task: input.task,
        arm_plan: input.arm_plan,
        status: input.status,
        claim_scope: input.claim_scope,
        patch_diff: input.patch_diff,
        transcript: input.transcript,
        tool_calls: input.tool_calls,
        command_records: input.command_records,
        context_injection: input.context_injection,
        wall_ms: input.started.elapsed().as_millis() as u64,
        cost_estimate: None,
    })
}

fn timing_json(started: Instant) -> Value {
    json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "total_elapsed_ms": started.elapsed().as_millis() as u64,
        "setup_ms": "unknown",
        "context_injection_ms": "unknown",
        "agent_ms": "unknown",
        "test_ms": "unknown",
        "scaffold_timing_only": true,
    })
}

fn quality_per_budget_json(arm_plan: &V1ArmRunPlan, status: &str) -> Value {
    json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "task_id": arm_plan.task_id,
        "mode": arm_plan.mode.as_str(),
        "status": status,
        "token_budget": arm_plan.token_budget,
        "tool_budget": arm_plan.tool_budget,
        "timeout_ms": arm_plan.timeout_ms,
        "quality_score": "unknown",
        "quality_per_token": "unknown",
        "quality_per_tool_call": "unknown",
        "quality_per_ms": "unknown",
        "real_agent_patch_quality_claim": false,
    })
}

fn patch_modes_status(artifacts: &[V1ArmArtifacts]) -> String {
    let patch_statuses = artifacts
        .iter()
        .filter(|artifact| {
            artifact.mode == V1AgentMode::RgOnlyAgentPatch.as_str()
                || artifact.mode == V1AgentMode::RgPlusCodegraphAgentPatch.as_str()
        })
        .map(|artifact| artifact.status.as_str())
        .collect::<BTreeSet<_>>();
    if patch_statuses.contains("blocked_external_agent_missing") {
        "blocked_external_agent_missing".to_string()
    } else if patch_statuses.contains("ready_for_real_patch_run") {
        "ready".to_string()
    } else if patch_statuses.is_empty() {
        "blocked_harness_missing".to_string()
    } else {
        "ready".to_string()
    }
}

fn root_manifest(artifacts: &V1SameAgentAbHarnessArtifacts) -> Value {
    let mut json_artifacts = vec![
        artifacts.run_plan_json.clone(),
        artifacts.blocked_tracks_json.clone(),
        artifacts.environment_json.clone(),
        artifacts.invariant_checks_json.clone(),
    ];
    let mut jsonl_artifacts = vec![artifacts.commands_jsonl.clone()];
    let mut required_artifacts = vec![
        required_artifact(&artifacts.run_plan_json, "root run plan"),
        required_artifact(&artifacts.commands_jsonl, "root command log"),
        required_artifact(&artifacts.blocked_tracks_json, "root blocked tracks"),
        required_artifact(&artifacts.environment_json, "root environment"),
        required_artifact(
            &artifacts.invariant_checks_json,
            "same-agent invariant checks",
        ),
    ];
    for arm in &artifacts.arm_artifacts {
        json_artifacts.extend([
            arm.run_plan_json.clone(),
            arm.agent_prompt_json.clone(),
            arm.agent_transcript_json.clone(),
            arm.context_injected_json.clone(),
            arm.score_json.clone(),
            arm.timing_breakdown_json.clone(),
            arm.quality_per_budget_json.clone(),
            arm.blocked_tracks_json.clone(),
            arm.environment_json.clone(),
            arm.artifact_manifest_json.clone(),
        ]);
        jsonl_artifacts.extend([arm.commands_jsonl.clone(), arm.tool_call_log_jsonl.clone()]);
        required_artifacts.extend([
            required_artifact(&arm.run_plan_json, "arm run plan"),
            required_artifact(&arm.commands_jsonl, "arm command log"),
            required_artifact(&arm.agent_prompt_md, "arm prompt markdown"),
            required_artifact(&arm.agent_prompt_json, "arm prompt json"),
            required_artifact(&arm.agent_transcript_md, "arm transcript markdown"),
            required_artifact(&arm.agent_transcript_json, "arm transcript json"),
            required_artifact(&arm.tool_call_log_jsonl, "arm tool-call log"),
            required_artifact(&arm.context_injected_json, "arm context injection record"),
            required_artifact(&arm.test_stdout, "arm test stdout"),
            required_artifact(&arm.test_stderr, "arm test stderr"),
            required_artifact(&arm.score_json, "arm score"),
            required_artifact(&arm.timing_breakdown_json, "arm timing"),
            required_artifact(&arm.quality_per_budget_json, "arm quality per budget"),
            required_artifact(&arm.blocked_tracks_json, "arm blocked tracks"),
            required_artifact(&arm.environment_json, "arm environment"),
            required_artifact(&arm.artifact_manifest_json, "arm artifact manifest"),
        ]);
        if let Some(patch_diff) = &arm.patch_diff {
            required_artifacts.push(required_artifact(patch_diff, "optional patch diff"));
        }
    }
    json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "harness": V1_AB_HARNESS_NAME,
        "run_id": artifacts.run_id,
        "run_root": artifacts.run_root,
        "required_artifacts": required_artifacts,
        "json_artifacts": json_artifacts,
        "jsonl_artifacts": jsonl_artifacts,
        "arm_artifacts": artifacts.arm_artifacts,
        "patch_modes_status": artifacts.patch_modes_status,
        "gold_leakage_audit_passed": artifacts.gold_leakage_audit_passed,
        "normal_dot_codegraph_mutated": artifacts.normal_dot_codegraph_mutated,
        "public_claim": false,
        "real_agent_patch_quality_claim": false,
    })
}

fn arm_manifest(artifact: &V1ArmArtifacts) -> Value {
    json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "task_id": artifact.task_id,
        "mode": artifact.mode,
        "arm": artifact.arm,
        "status": artifact.status,
        "claim_scope": artifact.claim_scope,
        "required_artifacts": [
            required_artifact(&artifact.run_plan_json, "run plan"),
            required_artifact(&artifact.commands_jsonl, "commands jsonl"),
            required_artifact(&artifact.agent_prompt_md, "agent prompt markdown"),
            required_artifact(&artifact.agent_prompt_json, "agent prompt json"),
            required_artifact(&artifact.agent_transcript_md, "agent transcript markdown"),
            required_artifact(&artifact.agent_transcript_json, "agent transcript json"),
            required_artifact(&artifact.tool_call_log_jsonl, "tool-call log"),
            required_artifact(&artifact.context_injected_json, "context injection"),
            required_artifact(&artifact.test_stdout, "test stdout"),
            required_artifact(&artifact.test_stderr, "test stderr"),
            required_artifact(&artifact.score_json, "score"),
            required_artifact(&artifact.timing_breakdown_json, "timing breakdown"),
            required_artifact(&artifact.quality_per_budget_json, "quality per budget"),
            required_artifact(&artifact.blocked_tracks_json, "blocked tracks"),
            required_artifact(&artifact.environment_json, "environment")
        ],
        "public_claim": false,
        "real_agent_patch_quality_claim": false,
    })
}

fn required_artifact(path: &str, label: &str) -> Value {
    json!({
        "path": path,
        "label": label,
        "validation_rule": "path exists and parses when JSON or JSONL",
        "repair_action_if_missing": "rerun the v1 same-agent A/B harness smoke for this task and mode"
    })
}

fn same_agent_contract_json() -> Value {
    json!({
        "a_arm": "same agent plus normal rg/search/edit/test tools",
        "b_arm": "same agent plus normal rg/search/edit/test tools plus CodeGraph",
        "fixed_equal_fields": [
            "model",
            "external_agent_command",
            "agent_scaffold_version",
            "system_prompt_base",
            "task_prompt_base_except_codegraph_context_injection",
            "repo_commit",
            "setup_command",
            "test_command",
            "timeout_ms",
            "token_budget",
            "tool_budget",
            "environment_allowlist",
            "docker_image",
            "evaluator_kind",
            "seed"
        ],
        "allowed_b_only_differences": [
            "codegraph_tools_context_available",
            "codegraph_context_injected",
            "codegraph_validation_agent_use_commands_available"
        ],
        "public_claim": false,
        "real_agent_patch_quality_claim": false
    })
}

fn environment_json(options: &V1SameAgentAbHarnessOptions) -> Value {
    let values = options
        .environment_allowlist
        .iter()
        .map(|name| {
            json!({
                "name": name,
                "present": env::var_os(name).is_some(),
                "value_recorded": name == CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV,
                "value": if name == CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV {
                    env::var(name).unwrap_or_else(|_| "unset".to_string())
                } else {
                    "redacted_or_not_recorded".to_string()
                }
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_version": V1_AB_HARNESS_SCHEMA_VERSION,
        "environment_allowlist": options.environment_allowlist,
        "values": values,
        "secrets_policy": "only allowlisted metadata is recorded; non-command values are redacted",
    })
}

fn render_list(values: &[String]) -> String {
    if values.is_empty() {
        return "- none".to_string();
    }
    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn validate_jsonl(path: &str) -> BenchResult<usize> {
    let contents = fs::read_to_string(path)?;
    let mut count = 0usize;
    for (line_index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(BenchmarkError::Validation(format!(
                "blank JSONL line {} in {path}",
                line_index + 1
            )));
        }
        serde_json::from_str::<Value>(line).map_err(|error| {
            BenchmarkError::Parse(format!(
                "invalid JSONL line {} in {path}: {error}",
                line_index + 1
            ))
        })?;
        count += 1;
    }
    Ok(count)
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn write_json(path: &Path, value: &impl Serialize) -> BenchResult<()> {
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

fn write_jsonl(path: &Path, values: &[Value]) -> BenchResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = String::new();
    for value in values {
        output.push_str(
            &serde_json::to_string(value)
                .map_err(|error| BenchmarkError::Parse(error.to_string()))?,
        );
        output.push('\n');
    }
    fs::write(path, output)?;
    Ok(())
}

fn write_text(path: &Path, contents: &str) -> BenchResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn command_records_to_values(records: &[V1CommandRecord]) -> BenchResult<Vec<Value>> {
    records
        .iter()
        .map(|record| {
            serde_json::to_value(record).map_err(|error| BenchmarkError::Parse(error.to_string()))
        })
        .collect()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn dot_codegraph_state(workspace_root: &Path) -> Option<(bool, u64)> {
    let path = workspace_root.join(".codegraph");
    fs::metadata(path)
        .map(|metadata| (metadata.is_dir(), metadata.len()))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn smoke_options(modes: Vec<V1AgentMode>) -> V1SameAgentAbHarnessOptions {
        let mut options = default_v1_same_agent_ab_harness_options();
        let suffix = TEST_RUN_COUNTER.fetch_add(1, Ordering::SeqCst);
        options.run_id = Some(format!(
            "v1-ab-harness-test-{}-{}-{suffix}",
            std::process::id(),
            unix_ms()
        ));
        options.run_root = Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target")
                .join("codegraph-bench-runs")
                .join(options.run_id.as_ref().expect("run id")),
        );
        if let Ok(root) = env::var("CODEGRAPH_BENCH_V1_AB_SMOKE_ROOT") {
            options.run_id = Some(format!(
                "v1-ab-harness-smoke-{}-{}-{suffix}",
                std::process::id(),
                unix_ms()
            ));
            options.run_root = Some(PathBuf::from(root).join(options.run_id.as_ref().unwrap()));
        }
        options.task_id = Some("local_wrong_file_trap_discount".to_string());
        options.external_agent_command = None;
        options.modes = modes;
        options
    }

    fn run_smoke(modes: Vec<V1AgentMode>) -> V1SameAgentAbHarnessArtifacts {
        run_v1_same_agent_ab_harness(smoke_options(modes)).expect("v1 A/B harness smoke")
    }

    fn run_smoke_with_context_scenario(
        modes: Vec<V1AgentMode>,
        scenario: V1CodeGraphContextScenario,
    ) -> V1SameAgentAbHarnessArtifacts {
        let mut options = smoke_options(modes);
        options.codegraph_context_scenario = scenario;
        run_v1_same_agent_ab_harness(options).expect("v1 context injection smoke")
    }

    #[test]
    fn v1_ab_invariant_equality_passes_for_plan_pair() {
        let artifacts = run_smoke(vec![
            V1AgentMode::RgOnlyAgentPlan,
            V1AgentMode::RgPlusCodegraphAgentPlan,
        ]);
        assert_eq!(artifacts.invariant_checks.len(), 1);
        assert!(artifacts.invariant_checks[0].passed);
        assert!(artifacts.invariant_checks[0]
            .observed_allowed_differences
            .contains(&"codegraph_available".to_string()));
    }

    #[test]
    fn v1_ab_invariant_rejects_model_drift() {
        let artifacts = run_smoke(vec![
            V1AgentMode::RgOnlyAgentPlan,
            V1AgentMode::RgPlusCodegraphAgentPlan,
        ]);
        let left: V1ArmRunPlan = serde_json::from_str(
            &fs::read_to_string(&artifacts.arm_artifacts[0].run_plan_json).expect("plan"),
        )
        .expect("left plan");
        let mut right: V1ArmRunPlan = serde_json::from_str(
            &fs::read_to_string(&artifacts.arm_artifacts[1].run_plan_json).expect("plan"),
        )
        .expect("right plan");
        right.model = "different_model".to_string();
        let check = check_v1_same_agent_invariants(&left, &right);
        assert!(!check.passed);
        assert!(check
            .mismatched_forbidden_fields
            .contains(&"model".to_string()));
    }

    #[test]
    fn v1_b_arm_differs_only_by_codegraph_availability_and_context() {
        let artifacts = run_smoke(vec![
            V1AgentMode::RgOnlyAgentPlan,
            V1AgentMode::RgPlusCodegraphAgentPlan,
        ]);
        let check = &artifacts.invariant_checks[0];
        assert!(check.passed);
        assert!(check.mismatched_forbidden_fields.is_empty());
        assert!(check
            .observed_allowed_differences
            .contains(&"codegraph_context_injected".to_string()));
        assert!(check
            .observed_allowed_differences
            .contains(&"codegraph_validation_commands_available".to_string()));
    }

    #[test]
    fn v1_agent_prompt_excludes_hidden_gold_fields() {
        let artifacts = run_smoke(vec![
            V1AgentMode::RgOnlyAgentPlan,
            V1AgentMode::RgPlusCodegraphAgentPlan,
        ]);
        for artifact in &artifacts.arm_artifacts {
            let prompt = fs::read_to_string(&artifact.agent_prompt_md).expect("agent prompt");
            let prompt_json = fs::read_to_string(&artifact.agent_prompt_json).expect("prompt json");
            assert!(!prompt.contains("src/payments/discounts.py"));
            assert!(!prompt.contains("calculate_checkout_discount"));
            assert!(!prompt.contains("tests/test_discounts.py"));
            assert!(!prompt_json.contains("src/payments/discounts.py"));
            assert!(!prompt_json.contains("calculate_checkout_discount"));
            assert!(!prompt_json.contains("tests/test_discounts.py"));
        }
    }

    #[test]
    fn v1_b_prompt_includes_codegraph_packet_and_a_prompt_does_not() {
        let artifacts = run_smoke(vec![
            V1AgentMode::RgOnlyAgentPlan,
            V1AgentMode::RgPlusCodegraphAgentPlan,
        ]);
        let a_prompt = fs::read_to_string(&artifacts.arm_artifacts[0].agent_prompt_json)
            .expect("A prompt JSON");
        let b_prompt = fs::read_to_string(&artifacts.arm_artifacts[1].agent_prompt_json)
            .expect("B prompt JSON");
        assert!(!a_prompt.contains("codegraph_context_packet"));
        assert!(b_prompt.contains("codegraph_context_packet"));
        let a_md =
            fs::read_to_string(&artifacts.arm_artifacts[0].agent_prompt_md).expect("A prompt");
        let b_md =
            fs::read_to_string(&artifacts.arm_artifacts[1].agent_prompt_md).expect("B prompt");
        assert!(!a_md.contains("## CodeGraph Context Packet"));
        assert!(b_md.contains("## CodeGraph Context Packet"));
    }

    #[test]
    fn v1_b_context_packet_records_proof_labels() {
        let artifacts = run_smoke(vec![V1AgentMode::RgPlusCodegraphAgentPlan]);
        let context: Value = serde_json::from_str(
            &fs::read_to_string(&artifacts.arm_artifacts[0].context_injected_json)
                .expect("context"),
        )
        .expect("context JSON");
        let packet = &context["packet"];
        assert_eq!(packet["codegraph_context_available"], true);
        assert_eq!(packet["graph_proof_available"], false);
        assert_eq!(packet["graph_proof_overclaim_count"], 0);
        assert!(packet["proof_boundary_summary"]
            .as_str()
            .unwrap_or("")
            .contains("Only graph/source"));
    }

    #[test]
    fn v1_stale_db_context_is_not_injected_as_fresh_graph_proof() {
        let artifacts = run_smoke_with_context_scenario(
            vec![V1AgentMode::RgPlusCodegraphAgentPlan],
            V1CodeGraphContextScenario::StaleGraphDb,
        );
        let context: Value = serde_json::from_str(
            &fs::read_to_string(&artifacts.arm_artifacts[0].context_injected_json)
                .expect("context"),
        )
        .expect("context JSON");
        let packet = &context["packet"];
        assert_eq!(packet["profile_db_status"], "repo_head_mismatch");
        assert_eq!(packet["db_claimable"], false);
        assert_eq!(packet["graph_proof_available"], false);
        assert_eq!(packet["stale_evidence_present"], true);
    }

    #[test]
    fn v1_context_byte_and_token_budgets_are_enforced_in_harness() {
        let mut options = smoke_options(vec![V1AgentMode::RgPlusCodegraphAgentPlan]);
        options.codegraph_context_budget = V1CodeGraphContextBudget {
            max_bytes: 2_500,
            max_tokens_estimate: 625,
        };
        let artifacts =
            run_v1_same_agent_ab_harness(options).expect("budgeted context harness smoke");
        let context: Value = serde_json::from_str(
            &fs::read_to_string(&artifacts.arm_artifacts[0].context_injected_json)
                .expect("context"),
        )
        .expect("context JSON");
        assert!(context["packet"]["context_packet_bytes"].as_u64().unwrap() <= 2_500);
        assert!(
            context["packet"]["context_packet_tokens_estimate"]
                .as_u64()
                .unwrap()
                <= 625
        );
    }

    #[test]
    fn v1_missing_external_agent_blocks_patch_modes() {
        let artifacts = run_smoke(vec![
            V1AgentMode::RgOnlyAgentPatch,
            V1AgentMode::RgPlusCodegraphAgentPatch,
        ]);
        assert_eq!(
            artifacts.patch_modes_status,
            "blocked_external_agent_missing"
        );
        assert!(artifacts
            .arm_artifacts
            .iter()
            .all(|artifact| artifact.status == "blocked_external_agent_missing"));
    }

    #[test]
    fn v1_mock_mode_is_labeled_scaffold_only() {
        let artifacts = run_smoke(vec![V1AgentMode::MockAgentScaffoldOnly]);
        let artifact = &artifacts.arm_artifacts[0];
        assert_eq!(artifact.status, "scaffold_only");
        assert_eq!(artifact.claim_scope, "scaffold_only");
        assert!(artifact.patch_diff.is_some());
        let score = fs::read_to_string(&artifact.score_json).expect("score");
        assert!(score.contains("\"real_agent_patch_quality_claim\": false"));
    }

    #[test]
    fn v1_plan_only_mode_is_labeled_diagnostic() {
        let artifacts = run_smoke(vec![V1AgentMode::PlanOnlyDiagnostic]);
        let artifact = &artifacts.arm_artifacts[0];
        assert_eq!(artifact.status, "plan_only_diagnostic");
        assert_eq!(artifact.claim_scope, "plan_only_diagnostic");
        assert!(artifact.patch_diff.is_none());
    }

    #[test]
    fn v1_command_logs_exact_argv() {
        let artifacts = run_smoke(vec![V1AgentMode::MockAgentScaffoldOnly]);
        let commands =
            fs::read_to_string(&artifacts.arm_artifacts[0].commands_jsonl).expect("commands");
        assert!(commands.contains("\"argv\""));
        assert!(commands.contains("mock_agent_scaffold_only"));
        assert!(commands.contains("--task-json"));
        validate_jsonl(&artifacts.arm_artifacts[0].commands_jsonl).expect("jsonl");
    }

    #[test]
    fn v1_run_plan_is_generated_before_execution() {
        let artifacts = run_smoke(vec![V1AgentMode::MockAgentScaffoldOnly]);
        let run_plan: Value = serde_json::from_str(
            &fs::read_to_string(&artifacts.arm_artifacts[0].run_plan_json).expect("run plan"),
        )
        .expect("run plan JSON");
        assert_eq!(run_plan["generated_before_execution"], true);
        assert!(Path::new(&artifacts.arm_artifacts[0].agent_transcript_json).exists());
    }

    #[test]
    fn v1_blocked_tracks_record_missing_external_agent() {
        let artifacts = run_smoke(vec![V1AgentMode::RgOnlyAgentPatch]);
        let blocked = fs::read_to_string(&artifacts.arm_artifacts[0].blocked_tracks_json)
            .expect("blocked tracks");
        assert!(blocked.contains(CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND_ENV));
        assert!(blocked.contains("blocked_external_agent_missing"));
    }

    #[test]
    fn v1_harness_does_not_mutate_normal_dot_codegraph() {
        let before = dot_codegraph_state(&workspace_root());
        let artifacts = run_smoke(vec![V1AgentMode::MockAgentScaffoldOnly]);
        let after = dot_codegraph_state(&workspace_root());
        assert_eq!(before, after);
        assert!(!artifacts.normal_dot_codegraph_mutated);
    }

    #[test]
    fn v1_json_artifacts_validate() {
        let artifacts = run_smoke(vec![
            V1AgentMode::RgOnlyAgentPlan,
            V1AgentMode::RgPlusCodegraphAgentPlan,
            V1AgentMode::RgOnlyAgentPatch,
            V1AgentMode::RgPlusCodegraphAgentPatch,
            V1AgentMode::MockAgentScaffoldOnly,
            V1AgentMode::PlanOnlyDiagnostic,
        ]);
        validate_v1_same_agent_ab_artifacts(Path::new(&artifacts.artifact_manifest_json))
            .expect("artifact manifest validates");
        assert!(artifacts.gold_leakage_audit_passed);
    }

    #[test]
    fn v1_codegraph_context_smoke_matrix_artifacts_validate() {
        let ab = run_smoke(vec![
            V1AgentMode::RgOnlyAgentPlan,
            V1AgentMode::RgPlusCodegraphAgentPlan,
        ]);
        validate_v1_same_agent_ab_artifacts(Path::new(&ab.artifact_manifest_json))
            .expect("A/B context smoke manifest");
        let a_prompt =
            fs::read_to_string(&ab.arm_artifacts[0].agent_prompt_json).expect("A prompt JSON");
        let b_prompt =
            fs::read_to_string(&ab.arm_artifacts[1].agent_prompt_json).expect("B prompt JSON");
        assert!(!a_prompt.contains("codegraph_context_packet"));
        assert!(b_prompt.contains("codegraph_context_packet"));

        let stale = run_smoke_with_context_scenario(
            vec![V1AgentMode::RgPlusCodegraphAgentPlan],
            V1CodeGraphContextScenario::StaleGraphDb,
        );
        validate_v1_same_agent_ab_artifacts(Path::new(&stale.artifact_manifest_json))
            .expect("stale context smoke manifest");
        let stale_context: Value = serde_json::from_str(
            &fs::read_to_string(&stale.arm_artifacts[0].context_injected_json).expect("context"),
        )
        .expect("stale context JSON");
        assert_eq!(
            stale_context["packet"]["profile_db_status"],
            "repo_head_mismatch"
        );

        let mut no_proof_options = smoke_options(vec![V1AgentMode::RgPlusCodegraphAgentPlan]);
        no_proof_options.task_id = Some("local_no_proof_fallback_runbook".to_string());
        no_proof_options.codegraph_context_scenario =
            V1CodeGraphContextScenario::CandidateOnlyNoProof;
        let no_proof =
            run_v1_same_agent_ab_harness(no_proof_options).expect("no-proof context smoke");
        validate_v1_same_agent_ab_artifacts(Path::new(&no_proof.artifact_manifest_json))
            .expect("no-proof context smoke manifest");
        let no_proof_context: Value = serde_json::from_str(
            &fs::read_to_string(&no_proof.arm_artifacts[0].context_injected_json).expect("context"),
        )
        .expect("no-proof context JSON");
        assert_eq!(
            no_proof_context["packet"]["codegraph_context_status"],
            "no_proof_path_found"
        );
        assert_eq!(no_proof_context["packet"]["graph_proof_available"], false);
    }

    #[test]
    fn v1_external_agent_command_contract_accepts_json_argv_and_placeholders() {
        let parsed = parse_external_agent_command(
            r#"["agent", "--task-json", "{task_json}", "--workspace", "{workspace}"]"#,
        )
        .expect("external argv");
        assert_eq!(parsed[0], "agent");
        let artifacts = run_smoke(vec![V1AgentMode::RgOnlyAgentPlan]);
        let plan: V1ArmRunPlan = serde_json::from_str(
            &fs::read_to_string(&artifacts.arm_artifacts[0].run_plan_json).expect("run plan"),
        )
        .expect("run plan");
        let argv = external_agent_argv(
            &plan,
            Path::new(&artifacts.arm_artifacts[0].artifact_dir),
            &parsed,
        );
        assert!(argv.iter().any(|part| part.ends_with("agent_prompt.json")));
        assert!(argv.iter().any(|part| part.contains("workspace")));
    }
}
