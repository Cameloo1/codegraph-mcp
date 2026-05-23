from __future__ import annotations

from enum import Enum
from typing import Any


class TrackStatus(str, Enum):
    READY_FOR_FULL_RUN = "ready_for_full_run"
    READY_FOR_RETRIEVAL_FULL_RUN = "ready_for_retrieval_full_run"
    READY_FOR_OFFICIAL_SMOKE = "ready_for_official_smoke"
    GOLD_VALIDATION_CACHED_READY = "gold_validation_cached_ready"
    GOLD_VALIDATION_LIVE_READY = "gold_validation_live_ready"
    PATCH_QUALITY_BLOCKED_MISSING_EXTERNAL_AGENT = "patch_quality_blocked_missing_external_agent"
    PATCH_QUALITY_BLOCKED_DOCKER = "patch_quality_blocked_docker"
    BLOCKED_DOCKER_LIVE_RUN = "blocked_docker_live_run"
    BLOCKED_QUERY_LEAKAGE = "blocked_query_leakage"
    NOT_CONFIGURED_BY_USER = "not_configured_by_user"
    SCAFFOLD_ONLY = "scaffold_only"
    UNAVAILABLE_MISSING_DATASET = "unavailable_missing_dataset"
    UNAVAILABLE_MISSING_BINARY = "unavailable_missing_binary"
    TIMED_OUT = "timed_out"
    COMPLETED = "completed"
    FAILED = "failed"
    OFFICIAL_SCORE_NOT_CLAIMED = "official_score_not_claimed"


TRACK_STATUS_VALUES = {status.value for status in TrackStatus}


def validate_track_status(value: str) -> bool:
    return value in TRACK_STATUS_VALUES


def classify_external_readiness(setup_summary: dict[str, Any], patch_summary: dict[str, Any]) -> dict[str, Any]:
    adapters = setup_summary.get("adapters", {}) if isinstance(setup_summary, dict) else {}
    internal = adapters.get("internal_gold", {})
    repobench = adapters.get("repobench", {})
    crosscodeeval = adapters.get("crosscodeeval", {})
    swe = adapters.get("swe_bench_lite", {})
    return {
        "internal_gold": _internal_status(internal),
        "repobench": _dataset_status(repobench),
        "crosscodeeval": _crosscodeeval_status(crosscodeeval),
        "swe_bench": classify_swe_readiness(swe, patch_summary),
    }


def classify_swe_readiness(swe_status: dict[str, Any], patch_summary: dict[str, Any]) -> dict[str, Any]:
    details = swe_status.get("details", {}) if isinstance(swe_status, dict) else {}
    docker = details.get("docker", {}) if isinstance(details, dict) else {}
    linux_harness = details.get("linux_harness", {}) if isinstance(details, dict) else {}
    python_import = details.get("python_import", {}) if isinstance(details, dict) else {}
    external = patch_summary.get("external_agent_command", {}) if isinstance(patch_summary, dict) else {}
    external_status = str(external.get("status", "blocked_not_configured"))
    docker_ready = docker.get("status") == "ready"
    python_ready = python_import.get("status") == "ready"
    cached_gold_ready = bool(linux_harness.get("gold_validation_report")) and linux_harness.get("status") == "ready"
    live_ready = bool(docker_ready and python_ready and swe_status.get("smoke_ready"))

    if external_status != "ready":
        patch_quality_status = TrackStatus.PATCH_QUALITY_BLOCKED_MISSING_EXTERNAL_AGENT.value
        patch_quality_reason = external.get("blocker") or "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND is not configured."
    elif not docker_ready:
        patch_quality_status = TrackStatus.PATCH_QUALITY_BLOCKED_DOCKER.value
        patch_quality_reason = docker.get("blocker") or "Docker is not ready."
    else:
        patch_quality_status = TrackStatus.READY_FOR_OFFICIAL_SMOKE.value
        patch_quality_reason = ""

    return {
        "gold_validation_cached": {
            "status": TrackStatus.GOLD_VALIDATION_CACHED_READY.value if cached_gold_ready else TrackStatus.NOT_CONFIGURED_BY_USER.value,
            "evidence_only": True,
            "counts_as_current_live_run": False,
            "report": linux_harness.get("gold_validation_report"),
        },
        "gold_validation_live": {
            "status": TrackStatus.GOLD_VALIDATION_LIVE_READY.value if live_ready else TrackStatus.BLOCKED_DOCKER_LIVE_RUN.value,
            "counts_as_current_live_run": live_ready,
            "blocked_reason": "" if live_ready else docker.get("blocker") or "Docker/Linux harness is not ready for a live gold-validation rerun.",
        },
        "patch_quality": {
            "status": patch_quality_status,
            "blocked_reason": patch_quality_reason,
            "external_agent_env": external.get("env", "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND"),
        },
        "mock_agent": {
            "status": TrackStatus.SCAFFOLD_ONLY.value,
            "quality_claim": False,
        },
        "official_score": {
            "status": TrackStatus.OFFICIAL_SCORE_NOT_CLAIMED.value,
            "public_claim": False,
        },
    }


def _internal_status(status: dict[str, Any]) -> dict[str, Any]:
    ready = bool(status.get("ready", True))
    return {
        "status": TrackStatus.READY_FOR_FULL_RUN.value if ready else TrackStatus.UNAVAILABLE_MISSING_DATASET.value,
        "task_count": status.get("task_count"),
    }


def _dataset_status(status: dict[str, Any]) -> dict[str, Any]:
    if status.get("ready"):
        return {"status": TrackStatus.READY_FOR_FULL_RUN.value, "details": status.get("details", {})}
    return {
        "status": TrackStatus.UNAVAILABLE_MISSING_DATASET.value,
        "blockers": status.get("blockers", []),
    }


def _crosscodeeval_status(status: dict[str, Any]) -> dict[str, Any]:
    if not status.get("ready"):
        return {
            "status": TrackStatus.UNAVAILABLE_MISSING_DATASET.value,
            "blockers": status.get("blockers", []),
        }
    if status.get("status") == TrackStatus.READY_FOR_OFFICIAL_SMOKE.value:
        return {"status": TrackStatus.READY_FOR_OFFICIAL_SMOKE.value, "details": status.get("details", {})}
    return {
        "status": TrackStatus.READY_FOR_RETRIEVAL_FULL_RUN.value,
        "details": status.get("details", {}),
        "blockers": status.get("blockers", []),
    }
