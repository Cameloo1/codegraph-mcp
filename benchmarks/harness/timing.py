from __future__ import annotations

from collections.abc import Iterable
from typing import Any


TIMING_BUCKETS = (
    "cold_db_build_ms",
    "vector_sidecar_build_ms",
    "warm_query_ms",
    "warm_context_pack_ms",
    "rg_ms",
    "codegraph_query_subprocess_ms",
    "codegraph_context_pack_subprocess_ms",
    "codegraph_index_subprocess_ms",
    "harness_scoring_ms",
    "harness_bookkeeping_ms",
    "harness_overhead_ms",
    "raw_total_ms",
    "cold_setup_excluded_total_ms",
    "end_to_end_first_use_ms",
)


def empty_timing_buckets() -> dict[str, int]:
    return {bucket: 0 for bucket in TIMING_BUCKETS}


def classify_timing_bucket(record: dict[str, Any]) -> list[str]:
    argv = [str(value).lower() for value in record.get("argv", [])]
    command_id = str(record.get("command_id", "")).lower()
    text = " ".join(argv)
    buckets: list[str] = []
    if "codegraph-mcp" in text and "index" in argv:
        buckets.extend(["cold_db_build_ms", "codegraph_index_subprocess_ms", "end_to_end_first_use_ms"])
        if "--build-vector-index" in argv:
            buckets.append("vector_sidecar_build_ms")
    elif "codegraph-mcp" in text and "context-pack" in argv:
        buckets.extend(["warm_context_pack_ms", "codegraph_context_pack_subprocess_ms", "cold_setup_excluded_total_ms"])
    elif "codegraph-mcp" in text and "query" in argv:
        buckets.extend(["warm_query_ms", "codegraph_query_subprocess_ms", "cold_setup_excluded_total_ms"])
    elif _is_rg(argv):
        buckets.extend(["rg_ms", "warm_query_ms", "cold_setup_excluded_total_ms"])
    elif "run_retrieval_eval" in text:
        buckets.append("harness_scoring_ms")
    elif "verify_benchmark_setup" in text or "unittest" in text or "run_patch_eval" in text:
        buckets.append("harness_bookkeeping_ms")
    elif "prebuild" in command_id:
        buckets.append("cold_db_build_ms")
    else:
        buckets.append("harness_overhead_ms")
    return buckets


def timing_breakdown(records: Iterable[dict[str, Any]], *, setup_paid_labels: list[str] | None = None) -> dict[str, Any]:
    buckets = empty_timing_buckets()
    command_records = []
    for record in records:
        wall = int(record.get("wall_time_ms") or 0)
        buckets["raw_total_ms"] += wall
        classified = classify_timing_bucket(record)
        for bucket in classified:
            if bucket in buckets and bucket != "raw_total_ms":
                buckets[bucket] += wall
        command_records.append(
            {
                "command_id": record.get("command_id"),
                "argv": record.get("argv", []),
                "wall_time_ms": wall,
                "buckets": classified,
                "success": record.get("success"),
                "failure_kind": record.get("failure_kind"),
            }
        )
    buckets["harness_overhead_ms"] = max(
        0,
        buckets["raw_total_ms"]
        - buckets["cold_db_build_ms"]
        - buckets["vector_sidecar_build_ms"]
        - buckets["warm_query_ms"]
        - buckets["warm_context_pack_ms"]
        - buckets["harness_scoring_ms"]
        - buckets["harness_bookkeeping_ms"],
    )
    return {
        "schema_version": "benchmark_timing_breakdown_v1",
        "buckets": buckets,
        "setup_paid_task_labels": setup_paid_labels or [],
        "notes": [
            "Cold DB build time is reported separately and is not hidden inside one task average.",
            "Warm retrieval excludes cold setup and includes repeated CLI subprocess cost where commands expose it.",
            "First-use timing is the operator-facing end-to-end setup plus first retrieval path.",
        ],
        "known_previous_comparison_shape": {
            "fresh_db_builds_s": 168.992,
            "codegraph_targeted_query_cli_calls_s": 67.426,
            "codegraph_context_pack_cli_calls_s": 31.284,
            "vector_sidecar_commands_s": 5.783,
            "rg_cli_calls_s": 1.165,
            "harness_scoring_bookkeeping_s": 66.870,
            "total_internal_run_s": 341.520,
        },
        "commands": command_records,
    }


def _is_rg(argv: list[str]) -> bool:
    if not argv:
        return False
    executable = argv[0].replace("\\", "/").split("/")[-1]
    return executable in {"rg", "rg.exe"}
