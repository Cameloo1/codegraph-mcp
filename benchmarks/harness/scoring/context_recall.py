from __future__ import annotations

from typing import Any


def normalize_path(path: str) -> str:
    return path.replace("\\", "/").lstrip("./")


def recall_at_k(gold: list[str], candidates: list[str], k: int) -> float | None:
    if not gold:
        return None
    gold_set = {normalize_path(item) for item in gold}
    cand_set = {normalize_path(item) for item in candidates[:k]}
    return len(gold_set & cand_set) / len(gold_set)


def mrr(gold: list[str], candidates: list[str]) -> float | None:
    if not gold:
        return None
    gold_set = {normalize_path(item) for item in gold}
    for index, candidate in enumerate(candidates, 1):
        if normalize_path(candidate) in gold_set:
            return 1.0 / index
    return 0.0


def score_retrieval(task: dict[str, Any], packet: dict[str, Any]) -> dict[str, Any]:
    files = [_file_value(value) for value in packet.get("files", [])]
    files = [value for value in files if value]
    symbols = [str(value) for value in packet.get("symbols", [])]
    spans = [str(value) for value in packet.get("spans", [])]
    return {
        "gold_file_recall_at_1": recall_at_k(task.get("gold_files", []), files, 1),
        "gold_file_recall_at_5": recall_at_k(task.get("gold_files", []), files, 5),
        "gold_file_recall_at_10": recall_at_k(task.get("gold_files", []), files, 10),
        "gold_symbol_recall_at_5": recall_at_k(task.get("gold_symbols", []), symbols, 5),
        "gold_span_recall_at_5": recall_at_k(task.get("gold_spans", []), spans, 5),
        "mrr": mrr(task.get("gold_files", []), files),
        "context_bytes": packet.get("raw_context_bytes", 0),
        "context_tokens_estimated": estimate_tokens(packet.get("raw_context_bytes", 0)),
        "time_to_first_useful_context_ms": None,
    }


def _file_value(value: Any) -> str:
    if isinstance(value, str):
        return normalize_path(value)
    if isinstance(value, dict):
        for key in ("file", "path", "file_path"):
            if value.get(key):
                return normalize_path(str(value[key]))
    return ""


def estimate_tokens(context_bytes: int | None) -> int | None:
    if context_bytes is None:
        return None
    return max(0, int(context_bytes / 4))

