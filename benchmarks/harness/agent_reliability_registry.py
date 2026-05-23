from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from benchmarks.harness.schema import (
    AGENT_RELIABILITY_TASK_FAMILIES,
    validate_agent_reliability_task,
)
from benchmarks.harness.task_sanitizer import sanitize_provider_task


DEFAULT_AGENT_RELIABILITY_REGISTRY = (
    Path(__file__).resolve().parents[1]
    / "datasets"
    / "agent_reliability"
    / "agent_reliability_tasks.jsonl"
)

REQUIRED_FIXTURE_TAGS = {
    "same_symbol_multiple_files",
    "stale_docs",
    "test_only_mocks",
    "generated_files",
    "deleted_renamed_files",
    "dynamic_dispatch",
    "config_driven_behavior",
    "no_proof_path",
    "same_filename_different_packages",
    "benchmark_query_leakage",
    "wrapper_exit_code_propagation",
    "stale_cache_live_readiness",
    "auth_boundary",
    "implementation_trace_artifact_math",
    "buildroot_planning",
    "codegraph_self_repo_db_lifecycle_debug",
}


def load_agent_reliability_tasks(path: Path | str = DEFAULT_AGENT_RELIABILITY_REGISTRY) -> list[dict[str, Any]]:
    registry_path = Path(path)
    tasks: list[dict[str, Any]] = []
    with registry_path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            stripped = line.strip()
            if not stripped:
                continue
            try:
                task = json.loads(stripped)
            except json.JSONDecodeError as exc:
                raise ValueError(f"{registry_path}:{line_number}: invalid JSONL: {exc}") from exc
            task["_registry_line"] = line_number
            tasks.append(task)
    return tasks


def provider_visible_agent_task(task: dict[str, Any]) -> dict[str, Any]:
    return sanitize_provider_task(task)


def validate_agent_reliability_registry(tasks: list[dict[str, Any]]) -> dict[str, Any]:
    errors: list[str] = []
    task_ids: set[str] = set()
    families: set[str] = set()
    fixture_tags: set[str] = set()

    for index, task in enumerate(tasks, 1):
        task_id = str(task.get("task_id") or f"line_{task.get('_registry_line', index)}")
        if task_id in task_ids:
            errors.append(f"duplicate task_id: {task_id}")
        task_ids.add(task_id)

        families.add(str(task.get("task_family") or ""))
        fixture_tags.update(_fixture_tags(task))

        for error in validate_agent_reliability_task(task):
            errors.append(f"{task_id}: {error}")

        provider_task = provider_visible_agent_task(task)
        leakage = provider_task.get("leakage_audit", {})
        if not leakage.get("pass"):
            errors.append(f"{task_id}: provider-visible leakage audit failed: {leakage}")

    missing_families = sorted(AGENT_RELIABILITY_TASK_FAMILIES - families)
    missing_fixture_tags = sorted(REQUIRED_FIXTURE_TAGS - fixture_tags)
    if missing_families:
        errors.append(f"missing task families: {missing_families}")
    if missing_fixture_tags:
        errors.append(f"missing fixture tags: {missing_fixture_tags}")

    return {
        "status": "pass" if not errors else "fail",
        "task_count": len(tasks),
        "task_families": sorted(families),
        "fixture_tags": sorted(fixture_tags),
        "errors": errors,
    }


def _fixture_tags(task: dict[str, Any]) -> set[str]:
    tags = set()
    for value in task.get("fixture_tags", []):
        if str(value).strip():
            tags.add(str(value))
    repo_fixture = task.get("repo_fixture")
    if isinstance(repo_fixture, str) and repo_fixture.strip():
        tags.add(repo_fixture.strip())
    return tags
