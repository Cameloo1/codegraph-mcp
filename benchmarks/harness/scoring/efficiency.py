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
            "context_bytes": _avg(items, ("retrieval", "context_bytes")),
            "tool_calls": _avg(items, ("efficiency", "tool_calls")),
            "wall_time_ms": _avg(items, ("efficiency", "wall_time_ms")),
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

