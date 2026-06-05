from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    tomllib = None  # type: ignore[assignment]

from benchmarks.harness.config import load_config, validate_config
from benchmarks.harness.runners.run_ab_agent_harness import (
    CODEGRAPH_TOOL,
    NORMAL_TOOLS,
    validate_same_agent_invariant,
)
from benchmarks.harness.runners.run_patch_eval import resolve_external_agent_command, validate_external_agent_command
from benchmarks.harness.schema import new_result, validate_result
from benchmarks.harness.task_sanitizer import EVALUATOR_ONLY_FIELDS, audit_provider_task


V1_CONFIG_SCHEMA_VERSION = "benchmark_v1_readiness_config_v1"
V1_TASK_MANIFEST_SCHEMA_VERSION = "benchmark_v1_task_manifest_template_v1"
V1_PREFLIGHT_SCHEMA_VERSION = "benchmark_v1_readiness_preflight_v1"
V1_RESULT_SCHEMA_MAP_VERSION = "benchmark_v1_result_schema_map_v1"

DEFAULT_CONFIG_PATH = Path("benchmarks/configs/benchmark_v1_readiness.template.toml")
DEFAULT_MANIFEST_PATH = Path("benchmarks/datasets/v1_readiness/task_manifest.template.json")

V1_REQUIRED_RESULT_FIELD_MAP = {
    "resolved_percentage": "patch.resolved_percentage",
    "test_pass_rate": "patch.test_pass_rate",
    "wrong_file_edits": "patch.wrong_file_edits",
    "nonexistent_symbol_references": "patch.nonexistent_symbol_refs",
    "unsupported_claims": "trust.unsupported_claim_count",
    "evidence_alignment": "patch.evidence_alignment",
    "time_ms": "efficiency.wall_time_ms",
    "tokens": "efficiency.total_tokens",
    "tool_calls": "efficiency.tool_calls",
    "context_bytes": "retrieval.context_bytes",
    "patch_size": "patch.patch_size_bytes",
    "retry_count": "patch.retry_count",
    "cost_per_solved_task": "efficiency.cost_per_solved_task_usd",
    "no_proof_behavior": "trust.no_proof_behavior_ok",
    "stale_db_behavior": "trust.stale_db_behavior",
    "context_attribution_validity": "patch.context_attribution_valid",
    "skipped_blocked_task_status": "patch.skipped_status",
    "public_claim": "public_claim",
}


def load_raw_toml(path: str | Path) -> dict[str, Any]:
    if tomllib is None:
        raise RuntimeError("tomllib is required; use Python 3.11+")
    with Path(path).open("rb") as handle:
        return tomllib.load(handle)


def validate_v1_config_template(path: str | Path = DEFAULT_CONFIG_PATH) -> dict[str, Any]:
    path = Path(path)
    errors: list[str] = []
    raw = load_raw_toml(path)
    base_config = load_config(path)
    errors.extend(validate_config(base_config))

    v1 = raw.get("v1", {})
    if v1.get("schema_version") != V1_CONFIG_SCHEMA_VERSION:
        errors.append("v1.schema_version must be benchmark_v1_readiness_config_v1")
    if not bool(v1.get("readiness_only")):
        errors.append("v1.readiness_only must be true")
    if bool(v1.get("patch_quality_runs_enabled")):
        errors.append("v1.patch_quality_runs_enabled must be false in the readiness template")
    if not bool(v1.get("require_explicit_patch_run")):
        errors.append("v1.require_explicit_patch_run must be true")
    if bool(v1.get("public_claim")):
        errors.append("v1.public_claim must be false")
    if bool(v1.get("real_patch_quality_claim")):
        errors.append("v1.real_patch_quality_claim must be false")
    if v1.get("arm_a_mode") != "rg_only_agent_patch":
        errors.append("v1.arm_a_mode must be rg_only_agent_patch")
    if v1.get("arm_b_mode") != "rg_plus_codegraph_agent_patch":
        errors.append("v1.arm_b_mode must be rg_plus_codegraph_agent_patch")
    if tuple(v1.get("normal_tools", [])) != NORMAL_TOOLS:
        errors.append("v1.normal_tools must be rg/search/edit/test")
    if v1.get("codegraph_extra_tools", []) != [CODEGRAPH_TOOL]:
        errors.append("v1.codegraph_extra_tools must contain only codegraph")
    for key in ("output_dir", "predictions_dir", "patches_dir", "logs_dir"):
        value = str(v1.get(key, ""))
        if not value.startswith("reports/audit/artifacts/benchmark_v1_readiness_scaffold/"):
            errors.append(f"v1.{key} must stay under reports/audit/artifacts/benchmark_v1_readiness_scaffold/")

    schema_map = raw.get("v1", {}).get("result_schema", {})
    for name, path_value in V1_REQUIRED_RESULT_FIELD_MAP.items():
        if schema_map.get(name) != path_value:
            errors.append(f"v1.result_schema.{name} must map to {path_value}")

    return {
        "schema_version": V1_CONFIG_SCHEMA_VERSION,
        "status": "pass" if not errors else "fail",
        "path": str(path),
        "base_config_valid": not validate_config(base_config),
        "errors": errors,
    }


def load_task_manifest(path: str | Path = DEFAULT_MANIFEST_PATH) -> dict[str, Any]:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def validate_v1_task_manifest(path: str | Path = DEFAULT_MANIFEST_PATH) -> dict[str, Any]:
    manifest = load_task_manifest(path)
    errors: list[str] = []
    if manifest.get("schema_version") != V1_TASK_MANIFEST_SCHEMA_VERSION:
        errors.append("schema_version must be benchmark_v1_task_manifest_template_v1")
    if manifest.get("public_claim") is not False:
        errors.append("public_claim must be false")
    if manifest.get("real_patch_quality_claim") is not False:
        errors.append("real_patch_quality_claim must be false")

    provider_fields = set(manifest.get("provider_visible_fields", []))
    evaluator_fields = set(manifest.get("evaluator_only_fields", []))
    overlap = sorted(provider_fields & evaluator_fields)
    if overlap:
        errors.append(f"provider/evaluator fields overlap: {overlap}")

    tasks = manifest.get("tasks", [])
    if not isinstance(tasks, list) or not tasks:
        errors.append("tasks must be a non-empty list")
    pinning_required = False
    leakage_failures = []
    for index, task in enumerate(tasks):
        if not isinstance(task, dict):
            errors.append(f"task {index} must be an object")
            continue
        for field in ("task_id", "repo", "visible", "evaluator_only", "claim_boundaries"):
            if field not in task:
                errors.append(f"{task.get('task_id', index)} missing {field}")
        repo = task.get("repo", {})
        if isinstance(repo, dict) and repo.get("pinning_required"):
            pinning_required = True
        visible = task.get("visible", {}) if isinstance(task.get("visible"), dict) else {}
        evaluator_only = task.get("evaluator_only", {}) if isinstance(task.get("evaluator_only"), dict) else {}
        forbidden_visible = sorted(key for key in visible if key in evaluator_fields or key in EVALUATOR_ONLY_FIELDS)
        if forbidden_visible:
            errors.append(f"{task.get('task_id', index)} visible contains evaluator-only fields: {forbidden_visible}")
        provider_task = provider_visible_v1_task(task)
        audit = provider_task.get("leakage_audit", {})
        if not audit.get("pass"):
            leakage_failures.append({"task_id": task.get("task_id", index), "audit": audit})
        if not isinstance(evaluator_only, dict):
            errors.append(f"{task.get('task_id', index)} evaluator_only must be an object")
    if not pinning_required and manifest.get("manifest_status") != "pinned":
        errors.append("task set must be pinned or clearly marked pinning_required")
    if leakage_failures:
        errors.append(f"provider-visible leakage failures: {leakage_failures}")

    return {
        "schema_version": V1_TASK_MANIFEST_SCHEMA_VERSION,
        "status": "pass" if not errors else "fail",
        "path": str(path),
        "task_count": len(tasks) if isinstance(tasks, list) else 0,
        "pinning_required": pinning_required,
        "provider_visible_leakage_pass": not leakage_failures,
        "errors": errors,
    }


def provider_visible_v1_task(task: dict[str, Any]) -> dict[str, Any]:
    visible = task.get("visible", {}) if isinstance(task.get("visible"), dict) else {}
    provider_task = {
        "task_id": task.get("task_id"),
        "prompt": visible.get("prompt", ""),
        "task": visible.get("prompt", ""),
        "visible_query_terms": list(visible.get("visible_query_terms", [])),
        "visible_file_hints": list(visible.get("visible_file_hints", [])),
        "visible_symbol_hints": list(visible.get("visible_symbol_hints", [])),
        "visible_text_hints": list(visible.get("visible_text_hints", [])),
        "visible_language": visible.get("visible_language", "unknown"),
        "visible_repo_metadata": dict(visible.get("visible_repo_metadata", {})),
    }
    provider_task["leakage_audit"] = audit_provider_task(provider_task, source_task=_source_task_for_audit(task))
    return provider_task


def v1_result_schema_scaffold() -> dict[str, Any]:
    result = new_result(
        run_id="benchmark-v1-readiness-schema",
        task_id="v1-template-pin-required-001",
        benchmark="swe_bench_lite",
        track="product_ablation",
        mode="rg_only",
        model="external",
        agent_scaffold="external_agent_command",
        context_provider="rg_only",
        dataset_version="PIN_REQUIRED_BEFORE_RUN",
        patch={
            "resolved_percentage": None,
            "test_pass_rate": None,
            "patch_size_bytes": None,
            "retry_count": None,
            "evidence_alignment": None,
            "context_attribution_valid": None,
            "skipped_status": "blocked",
            "blocked_reason": "patch_quality_blocked_missing_external_agent",
        },
        efficiency={
            "input_tokens": None,
            "output_tokens": None,
            "total_tokens": None,
            "tool_calls": None,
            "cost_per_solved_task_usd": None,
        },
        trust={
            "unsupported_claim_count": None,
            "no_proof_behavior_ok": None,
            "stale_db_behavior": None,
        },
    )
    result["public_claim"] = False
    result["real_patch_quality_claim"] = False
    return result


def validate_v1_result_schema() -> dict[str, Any]:
    result = v1_result_schema_scaffold()
    errors = validate_result(result)
    missing_mappings = []
    for name, dotted in V1_REQUIRED_RESULT_FIELD_MAP.items():
        if _lookup_dotted(result, dotted, missing := object()) is missing:
            missing_mappings.append({"field": name, "path": dotted})
    if missing_mappings:
        errors.append(f"missing v1 result field mappings: {missing_mappings}")
    try:
        json.dumps(result)
    except TypeError as exc:
        errors.append(f"result does not serialize to JSON: {exc}")
    return {
        "schema_version": V1_RESULT_SCHEMA_MAP_VERSION,
        "status": "pass" if not errors else "fail",
        "field_map": V1_REQUIRED_RESULT_FIELD_MAP,
        "errors": errors,
        "sample_result": result,
    }


def run_v1_preflight(
    *,
    config_path: str | Path = DEFAULT_CONFIG_PATH,
    manifest_path: str | Path = DEFAULT_MANIFEST_PATH,
    output_dir: str | Path = "reports/audit/artifacts/benchmark_v1_readiness_scaffold/preflight",
    patch_run_requested: bool = False,
    public_claim: bool = False,
    codegraph_available: bool | None = None,
    external_agent_command: str | None = None,
) -> dict[str, Any]:
    before_dot_codegraph = Path(".codegraph").exists()
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)
    config_validation = validate_v1_config_template(config_path)
    manifest_validation = validate_v1_task_manifest(manifest_path)
    result_schema_validation = validate_v1_result_schema()
    raw = load_raw_toml(config_path)
    v1 = raw.get("v1", {})
    agent_cfg = raw.get("agent", {})
    command, env_name, command_source = resolve_external_agent_command(agent_cfg, explicit_command=external_agent_command)
    external_agent = validate_external_agent_command(command, env_name, agent_cfg)
    external_agent_configured = bool(command and external_agent.get("configured"))

    if codegraph_available is None:
        codegraph_available = Path(str(raw.get("codegraph", {}).get("release_binary", "target/release/codegraph-mcp.exe"))).exists()
    rg_path = _resolve_rg()
    run_plan = _same_agent_run_plan(v1, manifest_path)
    same_agent = validate_same_agent_invariant(run_plan, results=None)
    ignored = {
        key: _git_check_ignore(str(v1.get(key, "")))
        for key in ("output_dir", "predictions_dir", "patches_dir", "logs_dir")
    }
    public_claim_blocked = public_claim or bool(v1.get("public_claim"))
    patch_runs_enabled = bool(v1.get("patch_quality_runs_enabled") and patch_run_requested and external_agent_configured)
    patch_status = "disabled_readiness_only"
    if patch_run_requested and not external_agent_configured:
        patch_status = "patch_quality_blocked_missing_external_agent"
    elif patch_run_requested and public_claim_blocked:
        patch_status = "blocked_public_claim_not_allowed"
    elif patch_runs_enabled:
        patch_status = "ready_but_not_run"
    elif not external_agent_configured:
        patch_status = "patch_quality_blocked_missing_external_agent"

    errors = []
    for name, validation in (
        ("config", config_validation),
        ("manifest", manifest_validation),
        ("result_schema", result_schema_validation),
    ):
        if validation.get("status") != "pass":
            errors.append(f"{name} validation failed")
    if not rg_path:
        errors.append("rg availability check failed")
    if not codegraph_available:
        errors.append("CodeGraph release binary is unavailable for B arm")
    if not same_agent.get("pass"):
        errors.append("same-agent A/B invariant failed")
    if not all(item.get("ignored") for item in ignored.values()):
        errors.append("one or more v1 output directories are not ignored/local")
    if public_claim_blocked:
        errors.append("public claim mode is not allowed in v1 readiness scaffold")
    if patch_runs_enabled:
        errors.append("patch-quality execution must not be enabled by default in this scaffold")

    after_dot_codegraph = Path(".codegraph").exists()
    normal_dot_codegraph_mutated = before_dot_codegraph != after_dot_codegraph
    if normal_dot_codegraph_mutated:
        errors.append("normal .codegraph state changed")

    summary = {
        "schema_version": V1_PREFLIGHT_SCHEMA_VERSION,
        "status": "pass" if not errors else "blocked",
        "public_claim": False,
        "real_patch_quality_claim": False,
        "config": config_validation,
        "task_manifest": manifest_validation,
        "result_schema": {
            "status": result_schema_validation["status"],
            "field_map": result_schema_validation["field_map"],
            "errors": result_schema_validation["errors"],
        },
        "external_agent": {
            "configured": external_agent_configured,
            "status": external_agent.get("status"),
            "env": env_name,
            "source": command_source,
            "validation": external_agent,
        },
        "patch_quality": {
            "runs_enabled": patch_runs_enabled,
            "run_requested": patch_run_requested,
            "status": patch_status,
            "real_patch_quality_claim": False,
        },
        "docker_official_harness": {
            "required": bool(v1.get("docker_official_harness_required")),
            "status": "not_executed_in_readiness_scaffold",
            "blocked_or_ready_source": "must be verified by the later explicit v1 execution prompt",
        },
        "same_agent_ab_invariant": same_agent,
        "rg_availability": {
            "required_for_both_arms": True,
            "status": "ready" if rg_path else "blocked",
            "path": rg_path,
        },
        "codegraph_availability": {
            "required_for_b_arm_only": True,
            "status": "ready" if codegraph_available else "blocked",
            "release_binary": str(raw.get("codegraph", {}).get("release_binary", "target/release/codegraph-mcp.exe")),
        },
        "mock_agent": {
            "status": "scaffold_only",
            "counts_as_model_quality": False,
            "counts_as_patch_quality": False,
            "counts_as_product_value": False,
        },
        "output_dirs_ignored_local": ignored,
        "public_claim_mode": {
            "available_by_default": False,
            "requested": public_claim,
            "status": "blocked" if public_claim_blocked else "disabled",
        },
        "normal_dot_codegraph_mutated": normal_dot_codegraph_mutated,
        "blocked_reasons": errors,
        "v1_status": "scaffold_ready_or_blocked_with_reason",
    }
    (output_path / "preflight_summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    return summary


def _source_task_for_audit(task: dict[str, Any]) -> dict[str, Any]:
    visible = task.get("visible", {}) if isinstance(task.get("visible"), dict) else {}
    evaluator = task.get("evaluator_only", {}) if isinstance(task.get("evaluator_only"), dict) else {}
    source_task = {
        "task_id": task.get("task_id"),
        "prompt": visible.get("prompt", ""),
        "visible_query_terms": list(visible.get("visible_query_terms", [])),
    }
    source_task.update(evaluator)
    return source_task


def _same_agent_run_plan(v1: dict[str, Any], manifest_path: str | Path) -> dict[str, Any]:
    fixed = {
        "model": "external",
        "agent_scaffold": str(v1.get("same_agent_scaffold", "external_agent_command")),
        "evaluator": str(v1.get("evaluator", "official_compatible_patch_plus_agent_reliability_scorers_v1")),
        "budget": {
            "timeout_s": None,
            "max_tool_calls": None,
            "max_context_bytes": None,
        },
    }
    return {
        "arms": {
            "A": {
                **fixed,
                "mode": v1.get("arm_a_mode"),
                "tools": list(NORMAL_TOOLS),
                "uses_codegraph": False,
                "task_manifest": str(manifest_path),
            },
            "B": {
                **fixed,
                "mode": v1.get("arm_b_mode"),
                "tools": list(NORMAL_TOOLS) + [CODEGRAPH_TOOL],
                "uses_codegraph": True,
                "task_manifest": str(manifest_path),
            },
        }
    }


def _resolve_rg() -> str | None:
    workspace_rg = Path(".codex-tools/rg.exe")
    if workspace_rg.exists():
        return str(workspace_rg)
    return shutil.which("rg")


def _git_check_ignore(path: str) -> dict[str, Any]:
    if not path:
        return {"ignored": False, "path": path, "reason": "empty path"}
    normalized = path.replace("\\", "/")
    try:
        proc = subprocess.run(["git", "check-ignore", path], text=True, capture_output=True, check=False)
    except FileNotFoundError as exc:
        if _known_ignored_local_artifact_path(normalized):
            return {
                "ignored": True,
                "path": path,
                "stdout": "",
                "stderr": str(exc),
                "fallback": "known local artifact prefix from repository ignore policy",
            }
        return {"ignored": False, "path": path, "stdout": "", "stderr": str(exc)}
    if proc.returncode != 0 and "dubious ownership" in proc.stderr and _known_ignored_local_artifact_path(normalized):
        return {
            "ignored": True,
            "path": path,
            "stdout": "",
            "stderr": proc.stderr.strip(),
            "fallback": "known local artifact prefix from repository ignore policy",
        }
    return {
        "ignored": proc.returncode == 0,
        "path": path,
        "stdout": proc.stdout.strip(),
        "stderr": proc.stderr.strip(),
    }


def _known_ignored_local_artifact_path(path: str) -> bool:
    prefixes = (
        "reports/audit/artifacts/",
        "benchmarks/results/",
        "benchmarks/workspaces/",
    )
    return any(path.startswith(prefix) for prefix in prefixes)


def _lookup_dotted(value: dict[str, Any], dotted: str, default: Any = None) -> Any:
    current: Any = value
    for part in dotted.split("."):
        if not isinstance(current, dict) or part not in current:
            return default
        current = current[part]
    return current


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", default=str(DEFAULT_CONFIG_PATH))
    parser.add_argument("--manifest", default=str(DEFAULT_MANIFEST_PATH))
    parser.add_argument("--output-dir", default="reports/audit/artifacts/benchmark_v1_readiness_scaffold/preflight")
    parser.add_argument("--request-patch-run", action="store_true")
    parser.add_argument("--public-claim", action="store_true")
    args = parser.parse_args(argv)
    summary = run_v1_preflight(
        config_path=args.config,
        manifest_path=args.manifest,
        output_dir=args.output_dir,
        patch_run_requested=args.request_patch_run,
        public_claim=args.public_claim,
    )
    print(json.dumps(summary, indent=2))
    return 0 if summary["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
