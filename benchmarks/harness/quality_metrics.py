from __future__ import annotations

from statistics import mean
from typing import Any


QUALITY_PER_BUDGET_KEYS = (
    "recall_per_1k_tokens",
    "mrr_per_1k_tokens",
    "recall_per_tool_call",
    "mrr_per_tool_call",
    "context_bytes_per_recalled_gold_file",
    "warm_ms_per_recalled_gold_file",
    "recall_per_1k_context_bytes",
    "mrr_per_1k_context_bytes",
    "files_recalled_per_second_warm",
    "files_recalled_per_tool_call",
)


def quality_per_budget(results: list[dict[str, Any]], timing_by_mode: dict[str, dict[str, Any]] | None = None) -> dict[str, Any]:
    grouped: dict[str, list[dict[str, Any]]] = {}
    for result in results:
        grouped.setdefault(str(result.get("mode", "unknown")), []).append(result)
    modes = {}
    for mode, items in grouped.items():
        timing = timing_by_mode.get(mode, {}) if timing_by_mode else {}
        warm_ms = _numeric(timing.get("warm_ms"))
        recall = _avg(items, ("retrieval", "gold_file_recall_at_5"))
        mrr = _avg(items, ("retrieval", "mrr"))
        tokens = _avg(items, ("retrieval", "context_tokens_estimated"))
        context_bytes = _avg(items, ("retrieval", "context_bytes"))
        tool_calls = _avg(items, ("efficiency", "tool_calls"))
        recalled_files = _avg_recalled_files(items)
        if warm_ms is None:
            warm_ms = _avg(items, ("efficiency", "wall_time_ms"))
        metrics = {
            "tasks": len(items),
            "recall_at_5": recall,
            "mrr": mrr,
            "context_tokens_estimated": tokens,
            "context_bytes": context_bytes,
            "tool_calls": tool_calls,
            "warm_ms": warm_ms,
            "recalled_gold_files_estimated": recalled_files,
            "recall_per_1k_tokens": _divide(recall, _per_1k(tokens)),
            "mrr_per_1k_tokens": _divide(mrr, _per_1k(tokens)),
            "recall_per_tool_call": _divide(recall, tool_calls),
            "mrr_per_tool_call": _divide(mrr, tool_calls),
            "context_bytes_per_recalled_gold_file": _divide(context_bytes, recalled_files),
            "warm_ms_per_recalled_gold_file": _divide(warm_ms, recalled_files),
            "recall_per_1k_context_bytes": _divide(recall, _per_1k(context_bytes)),
            "mrr_per_1k_context_bytes": _divide(mrr, _per_1k(context_bytes)),
            "files_recalled_per_second_warm": _divide(recalled_files, _seconds(warm_ms)),
            "files_recalled_per_tool_call": _divide(recalled_files, tool_calls),
        }
        modes[mode] = metrics
    return {
        "schema_version": "benchmark_quality_per_budget_v1",
        "modes": modes,
        "notes": [
            "These are local diagnostic efficiency metrics, not public benchmark claims.",
            "Higher Recall@5/MRR can still be more expensive in context bytes, tool calls, and wall time.",
        ],
        "comparison_notes": _comparison_notes(modes),
    }


def _avg(items: list[dict[str, Any]], path: tuple[str, ...]) -> float | None:
    values = []
    for item in items:
        current: Any = item
        for key in path:
            if not isinstance(current, dict):
                current = None
                break
            current = current.get(key)
        numeric = _numeric(current)
        if numeric is not None:
            values.append(numeric)
    return mean(values) if values else None


def _avg_recalled_files(items: list[dict[str, Any]]) -> float | None:
    values = []
    for item in items:
        explicit = _get(item, ("retrieval", "gold_files_recalled_at_5"))
        if isinstance(explicit, (int, float)):
            values.append(float(explicit))
            continue
        recall = _get(item, ("retrieval", "gold_file_recall_at_5"))
        if isinstance(recall, (int, float)):
            gold_count = _get(item, ("retrieval", "gold_file_count"))
            if isinstance(gold_count, (int, float)) and gold_count > 0:
                values.append(float(recall) * float(gold_count))
            else:
                values.append(float(recall))
    return mean(values) if values else None


def _comparison_notes(modes: dict[str, dict[str, Any]]) -> list[str]:
    notes = []
    rg = modes.get("rg_only")
    codegraph_modes = [item for name, item in modes.items() if name.startswith("codegraph")]
    if rg and codegraph_modes:
        best_codegraph = max(codegraph_modes, key=lambda item: item.get("recall_at_5") or -1)
        if (best_codegraph.get("recall_at_5") or 0) > (rg.get("recall_at_5") or 0):
            costs_more = []
            for key in ("context_bytes", "tool_calls", "warm_ms"):
                if (best_codegraph.get(key) or 0) > (rg.get(key) or 0):
                    costs_more.append(key)
            if costs_more:
                notes.append(
                    "A CodeGraph mode has better Recall@5 than rg_only in this local run, "
                    f"but spends more {', '.join(costs_more)}."
                )
    return notes


def _get(item: dict[str, Any], path: tuple[str, ...]) -> Any:
    current: Any = item
    for key in path:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current


def _numeric(value: Any) -> float | None:
    if isinstance(value, (int, float)):
        return float(value)
    return None


def _divide(numerator: float | None, denominator: float | None) -> float | None:
    if numerator is None or denominator is None or denominator <= 0:
        return None
    return numerator / denominator


def _per_1k(value: float | None) -> float | None:
    if value is None:
        return None
    return value / 1000.0


def _seconds(ms: float | None) -> float | None:
    if ms is None:
        return None
    return ms / 1000.0
