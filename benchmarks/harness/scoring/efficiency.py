from __future__ import annotations

from statistics import mean
from typing import Any


def aggregate_by_mode(results: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    grouped: dict[str, list[dict[str, Any]]] = {}
    for result in results:
        grouped.setdefault(result.get("mode", "unknown"), []).append(result)
    summary: dict[str, dict[str, Any]] = {}
    for mode, items in grouped.items():
        summary[mode] = {
            "tasks": len(items),
            "gold_file_recall_at_5": _avg(items, ("retrieval", "gold_file_recall_at_5")),
            "mrr": _avg(items, ("retrieval", "mrr")),
            "precision_at_5": _avg(items, ("retrieval", "precision_at_5")),
            "wrong_context_rate_at_5": _avg(items, ("retrieval", "wrong_context_rate_at_5")),
            "context_poison_count": _avg(items, ("retrieval", "context_poison_count")),
            "dangerous_context_present_rate": _bool_rate(items, ("retrieval", "dangerous_context_present")),
            "ndcg_at_5": _avg(items, ("retrieval", "ndcg_at_5")),
            "role_coverage": _avg(items, ("retrieval", "role_coverage")),
            "useful_file_diversity": _avg(items, ("retrieval", "useful_file_diversity")),
            "output_lines": _avg(items, ("retrieval", "output_lines")),
            "flood_events": _avg(items, ("retrieval", "flood_events")),
            "routing_packet_usefulness": _avg(items, ("retrieval", "routing_packet_usefulness")),
            "implementation_trace_usefulness": _avg(items, ("retrieval", "implementation_trace_usefulness")),
            "artifact_db_inspection_requirement_correct_rate": _bool_rate(items, ("retrieval", "artifact_db_inspection_requirement_correct")),
            "context_bytes": _avg(items, ("retrieval", "context_bytes")),
            "tool_calls": _avg(items, ("efficiency", "tool_calls")),
            "wall_time_ms": _avg(items, ("efficiency", "wall_time_ms")),
            "cold_setup_time_ms": _avg(items, ("efficiency", "cold_setup_time_ms")),
            "warm_retrieval_time_ms": _avg(items, ("efficiency", "warm_retrieval_time_ms")),
            "codegraph_query_subprocess_ms": _avg(items, ("efficiency", "codegraph_query_subprocess_ms")),
            "codegraph_context_pack_subprocess_ms": _avg(items, ("efficiency", "codegraph_context_pack_subprocess_ms")),
            "codegraph_index_subprocess_ms": _avg(items, ("efficiency", "codegraph_index_subprocess_ms")),
            "claimability_violations": sum(int(_get(item, ("trust", "claimability_violations")) or 0) for item in items),
            "unsupported_claim_violations": sum(int(_get(item, ("trust", "unsupported_claim_violations")) or 0) for item in items),
        }
    return summary


def _get(item: dict[str, Any], path: tuple[str, ...]):
    current: Any = item
    for key in path:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current


def _avg(items: list[dict[str, Any]], path: tuple[str, ...]) -> float | None:
    values = [_get(item, path) for item in items]
    numeric = [float(value) for value in values if isinstance(value, (int, float))]
    return mean(numeric) if numeric else None


def _bool_rate(items: list[dict[str, Any]], path: tuple[str, ...]) -> float | None:
    values = [_get(item, path) for item in items]
    bools = [bool(value) for value in values if isinstance(value, bool)]
    return mean(1.0 if value else 0.0 for value in bools) if bools else None
