from __future__ import annotations

import json
from pathlib import Path
from typing import Any


DEPENDENCY_MANIFEST_SCHEMA_VERSION = "benchmark_v05_dependency_manifest_v1"
DEPENDENCY_PREFLIGHT_SCHEMA_VERSION = "benchmark_v05_dependency_preflight_v1"
DEFAULT_MANIFEST_PATH = Path(__file__).with_name("benchmark_v05_dependency_manifest.json")


def load_dependency_manifest(path: str | Path = DEFAULT_MANIFEST_PATH) -> dict[str, Any]:
    manifest_path = Path(path)
    with manifest_path.open("r", encoding="utf-8") as handle:
        manifest = json.load(handle)
    if manifest.get("schema_version") != DEPENDENCY_MANIFEST_SCHEMA_VERSION:
        raise ValueError("dependency manifest schema_version must be benchmark_v05_dependency_manifest_v1")
    return manifest


def check_required_artifacts(
    manifest_path: str | Path = DEFAULT_MANIFEST_PATH,
    *,
    root: str | Path = ".",
) -> dict[str, Any]:
    """Evaluate the v0/v0.5 transition-gate artifact contract.

    This preflight is intentionally read-only. Repair and normalization happen in
    explicit phase prompts so a final gate cannot silently invent evidence.
    """

    root_path = Path(root)
    manifest = load_dependency_manifest(manifest_path)
    artifacts = list(manifest.get("artifacts", []))
    artifacts_by_path = {
        str(artifact.get("required_artifact_path")): artifact
        for artifact in artifacts
        if artifact.get("required_artifact_path")
    }
    final_required = [str(path) for path in manifest.get("final_gate_required_artifacts", [])]

    errors: list[str] = []
    missing_artifacts: list[dict[str, Any]] = []
    evaluated: list[dict[str, Any]] = []

    for required_path in final_required:
        if required_path not in artifacts_by_path:
            errors.append(f"final gate requires {required_path}, but it is not listed in the dependency manifest")

    for artifact in artifacts:
        result = _evaluate_artifact(artifact, root=root_path)
        evaluated.append(result)

        if result.get("blocking_if_missing") and not result.get("producing_phase"):
            errors.append(f"{result['required_artifact_path']} is required but has no producing_phase")

        if result.get("blocking_if_missing") and not result.get("exists"):
            missing_artifacts.append(
                {
                    "required_artifact_path": result["required_artifact_path"],
                    "producing_phase": result.get("producing_phase") or "unknown",
                    "repair_action": result.get("repair_action") or "unknown",
                    "substitute_artifacts": result.get("substitute_artifacts", []),
                    "reason": "required artifact is absent",
                }
            )
            if not result.get("repair_action"):
                errors.append(f"{result['required_artifact_path']} is missing and has no repair_action")

        if result.get("required_artifact_kind") == "json" and result.get("exists") and not result.get("json_valid"):
            errors.append(f"{result['required_artifact_path']} exists but is not valid JSON")

    return {
        "schema_version": DEPENDENCY_PREFLIGHT_SCHEMA_VERSION,
        "status": "pass" if not errors and not missing_artifacts else "blocked",
        "public_claim": False,
        "real_patch_quality_claim": False,
        "manifest_path": str(Path(manifest_path)),
        "final_gate_required_artifacts": final_required,
        "missing_artifacts": missing_artifacts,
        "errors": errors,
        "artifacts": evaluated,
    }


def validate_final_gate_completion(preflight: dict[str, Any], final_gate_report: dict[str, Any]) -> list[str]:
    if final_gate_report.get("status") == "complete" and preflight.get("status") != "pass":
        return ["final gate cannot be complete while hard required artifacts are missing or invalid"]
    return []


def _evaluate_artifact(artifact: dict[str, Any], *, root: Path) -> dict[str, Any]:
    path = root / str(artifact.get("required_artifact_path", ""))
    exists = path.exists()
    json_valid = "not_applicable"
    status_value: Any = "unknown"
    ready_value: Any = "unknown"

    if artifact.get("required_artifact_kind") == "json":
        json_valid = False
        if exists:
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
                json_valid = True
                status_field = artifact.get("expected_status_field")
                ready_field = artifact.get("expected_ready_field")
                if status_field:
                    status_value = _lookup_dotted(data, str(status_field), default="unknown")
                if ready_field:
                    ready_value = _lookup_dotted(data, str(ready_field), default="unknown")
            except (OSError, json.JSONDecodeError, UnicodeDecodeError):
                json_valid = False

    result = dict(artifact)
    result.update(
        {
            "exists": exists,
            "json_valid": json_valid,
            "status_value": status_value,
            "ready_to_move_on_value": ready_value,
        }
    )
    return result


def _lookup_dotted(value: dict[str, Any], dotted: str, *, default: Any = None) -> Any:
    current: Any = value
    for part in dotted.split("."):
        if not isinstance(current, dict) or part not in current:
            return default
        current = current[part]
    return current
