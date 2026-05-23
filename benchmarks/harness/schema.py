from __future__ import annotations

from copy import deepcopy
from typing import Any

RESULT_SCHEMA_VERSION = "benchmark_result_v1"

BENCHMARKS = {
    "internal",
    "repobench",
    "crosscodeeval",
    "swe_bench_lite",
    "swe_bench_verified",
    "swe_bench_live",
}

TRACKS = {
    "official",
    "official_compatible",
    "product_ablation",
    "product_ablation_diagnostic",
    "retrieval_context_diagnostic",
    "diagnostic",
    "scaffold_only",
}
MODES = {
    "baseline",
    "none",
    "rg_only",
    "rg_planned",
    "codegraph_exact_text",
    "codegraph_full",
    "codegraph_current",
    "codegraph_planned",
}


RESULT_TEMPLATE: dict[str, Any] = {
    "schema_version": RESULT_SCHEMA_VERSION,
    "run_id": "",
    "task_id": "",
    "benchmark": "internal",
    "track": "product_ablation",
    "mode": "baseline",
    "model": "none",
    "agent_scaffold": "retrieval_only",
    "context_provider": "",
    "repo": "",
    "repo_commit": "",
    "dataset_version": "",
    "budget": {
        "max_time_s": None,
        "max_tool_calls": None,
        "max_input_tokens": None,
        "max_output_tokens": None,
    },
    "retrieval": {
        "gold_file_recall_at_1": None,
        "gold_file_recall_at_5": None,
        "gold_file_recall_at_10": None,
        "gold_symbol_recall_at_5": None,
        "gold_span_recall_at_5": None,
        "ndcg_at_5": None,
        "ndcg_at_10": None,
        "gold_files_recalled_at_5": None,
        "gold_file_count": None,
        "mrr": None,
        "precision_at_1": None,
        "precision_at_5": None,
        "precision_at_10": None,
        "non_gold_top5_count": None,
        "non_gold_top10_count": None,
        "wrong_context_rate_at_5": None,
        "wrong_context_rate_at_10": None,
        "gold_density_in_context": None,
        "role_coverage": None,
        "useful_file_diversity": None,
        "output_lines": None,
        "flood_events": None,
        "routing_packet_usefulness": None,
        "implementation_trace_usefulness": None,
        "artifact_db_inspection_requirement_correct": None,
        "context_bytes_per_gold_hit": None,
        "context_bytes_per_recalled_gold_file": None,
        "returned_files_count": None,
        "returned_symbols_count": None,
        "forbidden_file_hits_at_5": None,
        "forbidden_file_hits_at_10": None,
        "forbidden_symbol_hits_at_5": None,
        "forbidden_symbol_hits_at_10": None,
        "context_poison_count": None,
        "context_poison_rate": None,
        "first_forbidden_rank": None,
        "forbidden_context_bytes": None,
        "dangerous_context_present": None,
        "context_bytes": None,
        "context_tokens_estimated": None,
        "time_to_first_useful_context_ms": None,
    },
    "patch": {
        "resolved": None,
        "tests_passed": None,
        "patch_applied": None,
        "wrong_file_edits": None,
        "nonexistent_symbol_refs": None,
    },
    "efficiency": {
        "wall_time_ms": None,
        "cold_setup_time_ms": None,
        "warm_retrieval_time_ms": None,
        "codegraph_query_subprocess_ms": None,
        "codegraph_context_pack_subprocess_ms": None,
        "codegraph_index_subprocess_ms": None,
        "output_lines": None,
        "tool_calls": None,
        "codegraph_calls": None,
        "rg_calls": None,
        "cost_usd": None,
    },
    "trust": {
        "claimability_violations": 0,
        "unsupported_claim_violations": 0,
        "no_proof_behavior_ok": None,
        "stale_db_blocked": None,
    },
    "artifacts": {
        "raw_log": "",
        "prediction": None,
        "patch": None,
        "context_packet": None,
    },
}


def new_result(**overrides: Any) -> dict[str, Any]:
    result = deepcopy(RESULT_TEMPLATE)
    for key, value in overrides.items():
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key].update(value)
        else:
            result[key] = value
    return result


def validate_result(result: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if result.get("schema_version") != RESULT_SCHEMA_VERSION:
        errors.append("schema_version must be benchmark_result_v1")
    for field in ("run_id", "task_id", "benchmark", "track", "mode"):
        if field not in result:
            errors.append(f"missing required field: {field}")
    if result.get("benchmark") not in BENCHMARKS:
        errors.append(f"invalid benchmark: {result.get('benchmark')}")
    if result.get("track") not in TRACKS:
        errors.append(f"invalid track: {result.get('track')}")
    if result.get("mode") not in MODES:
        errors.append(f"invalid mode: {result.get('mode')}")
    for object_field in ("budget", "retrieval", "patch", "efficiency", "trust", "artifacts"):
        if not isinstance(result.get(object_field), dict):
            errors.append(f"{object_field} must be an object")
    trust = result.get("trust") if isinstance(result.get("trust"), dict) else {}
    if trust.get("claimability_violations", 0) not in (0, None):
        errors.append("result contains claimability violations")
    if trust.get("unsupported_claim_violations", 0) not in (0, None):
        errors.append("result contains unsupported claim violations")
    return errors


def validate_task(task: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    for field in ("task_id", "repo_kind", "task", "gold_files", "gold_symbols", "gold_spans"):
        if field not in task:
            errors.append(f"missing task field: {field}")
    for list_field in ("gold_files", "gold_symbols", "gold_spans", "forbidden_files", "forbidden_symbols"):
        if list_field in task and not isinstance(task[list_field], list):
            errors.append(f"{list_field} must be a list")
    if "expected_claimability" in task and not isinstance(task["expected_claimability"], dict):
        errors.append("expected_claimability must be an object")
    return errors
