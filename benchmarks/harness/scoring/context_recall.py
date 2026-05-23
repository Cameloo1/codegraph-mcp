from __future__ import annotations

import math
from typing import Any


def normalize_path(path: str) -> str:
    return path.replace("\\", "/").lstrip("./")


def recall_at_k(gold: list[str], candidates: list[str], k: int) -> float | None:
    if not gold:
        return None
    gold_set = {normalize_path(item) for item in gold}
    cand_set = {normalize_path(item) for item in candidates[:k]}
    return len(gold_set & cand_set) / len(gold_set)


def hits_at_k(gold: list[str], candidates: list[str], k: int) -> int | None:
    if not gold:
        return None
    gold_set = {normalize_path(item) for item in gold}
    return len(gold_set & {normalize_path(item) for item in candidates[:k]})


def precision_at_k(gold: list[str], candidates: list[str], k: int) -> float | None:
    if not gold:
        return None
    gold_set = {normalize_path(item) for item in gold}
    return len(gold_set & {normalize_path(item) for item in candidates[:k]}) / k


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
    gold_files = task.get("gold_files", [])
    gold_symbols = task.get("gold_symbols", [])
    gold_spans = task.get("gold_spans", [])
    context_bytes = packet.get("raw_context_bytes", 0)
    file_hits_at_5 = hits_at_k(gold_files, files, 5)
    poison = _poison_metrics(task, packet, files, symbols)
    role_metrics = _role_metrics(task, packet)
    extra = _v05_metrics(task, packet, files, symbols)
    return {
        "gold_file_recall_at_1": recall_at_k(gold_files, files, 1),
        "gold_file_recall_at_5": recall_at_k(gold_files, files, 5),
        "gold_file_recall_at_10": recall_at_k(gold_files, files, 10),
        "gold_symbol_recall_at_5": recall_at_k(gold_symbols, symbols, 5),
        "gold_span_recall_at_5": recall_at_k(gold_spans, spans, 5),
        "ndcg_at_5": ndcg_at_k(gold_files, files, 5),
        "ndcg_at_10": ndcg_at_k(gold_files, files, 10),
        "gold_files_recalled_at_5": file_hits_at_5,
        "gold_file_count": len(gold_files),
        "mrr": mrr(gold_files, files),
        "precision_at_1": precision_at_k(gold_files, files, 1),
        "precision_at_5": precision_at_k(gold_files, files, 5),
        "precision_at_10": precision_at_k(gold_files, files, 10),
        "non_gold_top5_count": _non_gold_count(gold_files, files, 5),
        "non_gold_top10_count": _non_gold_count(gold_files, files, 10),
        "wrong_context_rate_at_5": _wrong_context_rate(task, files, 5),
        "wrong_context_rate_at_10": _wrong_context_rate(task, files, 10),
        "gold_density_in_context": _gold_density(gold_files, files),
        "context_bytes_per_gold_hit": _divide(context_bytes, file_hits_at_5),
        "context_bytes_per_recalled_gold_file": _divide(context_bytes, file_hits_at_5),
        "returned_files_count": len(files),
        "returned_symbols_count": len(symbols),
        "context_bytes": context_bytes,
        "context_tokens_estimated": estimate_tokens(context_bytes),
        "time_to_first_useful_context_ms": None,
        **role_metrics,
        **extra,
        **poison,
    }


def ndcg_at_k(gold: list[str], candidates: list[str], k: int) -> float | None:
    if not gold:
        return None
    gold_set = {normalize_path(item) for item in gold}
    dcg = 0.0
    for index, candidate in enumerate(candidates[:k], 1):
        if normalize_path(candidate) in gold_set:
            dcg += 1.0 / math.log2(index + 1)
    ideal_hits = min(len(gold_set), k)
    idcg = sum(1.0 / math.log2(index + 1) for index in range(1, ideal_hits + 1))
    return dcg / idcg if idcg else None


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


def _role_metrics(task: dict[str, Any], packet: dict[str, Any]) -> dict[str, Any]:
    expected_roles = [str(value) for value in task.get("expected_evidence_roles", []) if str(value).strip()]
    observed_roles = _observed_roles(packet)
    if not expected_roles:
        coverage = None
        missing = []
    else:
        expected = {role.lower() for role in expected_roles}
        observed = {role.lower() for role in observed_roles}
        missing = sorted(expected - observed)
        coverage = len(expected & observed) / len(expected) if expected else None
    return {
        "role_coverage": coverage,
        "expected_evidence_roles": expected_roles,
        "observed_evidence_roles": sorted(observed_roles),
        "missing_evidence_roles": missing,
    }


def _v05_metrics(task: dict[str, Any], packet: dict[str, Any], files: list[str], symbols: list[str]) -> dict[str, Any]:
    raw = packet.get("raw") if isinstance(packet.get("raw"), dict) else {}
    output_lines = _output_lines(packet)
    flood_events = _flood_events(packet)
    routing_usefulness = _routing_packet_usefulness(task, packet)
    implementation_usefulness = _implementation_trace_usefulness(task, packet, files, symbols)
    artifact_correct = _artifact_db_requirement_correct(task, packet)
    return {
        "useful_file_diversity": _useful_file_diversity(files),
        "output_lines": output_lines,
        "flood_events": flood_events,
        "evidence_alignment": _evidence_alignment(task, files, symbols),
        "routing_packet_usefulness": routing_usefulness,
        "implementation_trace_usefulness": implementation_usefulness,
        "artifact_db_inspection_requirement_correct": artifact_correct,
        "cold_setup_time_ms": _raw_timing(raw, "index_prebuild_ms"),
        "warm_retrieval_time_ms": _raw_timing(raw, "query_subprocess_ms") + _raw_timing(raw, "context_pack_subprocess_ms"),
        "codegraph_query_subprocess_ms": _raw_timing(raw, "query_subprocess_ms"),
        "codegraph_context_pack_subprocess_ms": _raw_timing(raw, "context_pack_subprocess_ms"),
        "codegraph_index_subprocess_ms": _raw_timing(raw, "index_prebuild_ms"),
    }


def _non_gold_count(gold: list[str], candidates: list[str], k: int) -> int | None:
    if not gold:
        return None
    gold_set = {normalize_path(item) for item in gold}
    return sum(1 for item in candidates[:k] if normalize_path(item) not in gold_set)


def _wrong_context_rate(task: dict[str, Any], candidates: list[str], k: int) -> float | None:
    gold = task.get("gold_files", [])
    if not gold:
        return None
    gold_set = {normalize_path(item) for item in gold}
    allowed = {normalize_path(item) for item in task.get("allowed_files", [])}
    wrong = sum(1 for item in candidates[:k] if normalize_path(item) not in gold_set and normalize_path(item) not in allowed)
    return wrong / k


def _gold_density(gold: list[str], candidates: list[str]) -> float | None:
    if not gold or not candidates:
        return None
    gold_set = {normalize_path(item) for item in gold}
    return sum(1 for item in candidates if normalize_path(item) in gold_set) / len(candidates)


def _observed_roles(packet: dict[str, Any]) -> set[str]:
    roles: set[str] = set()
    for key in ("snippets", "text_evidence", "proof_paths", "validation_steps"):
        values = packet.get(key, [])
        if not isinstance(values, list):
            continue
        for item in values:
            if not isinstance(item, dict):
                continue
            for field in ("role", "evidence_role", "proof_strength", "evidence_type", "planning_role"):
                value = item.get(field)
                if value:
                    roles.add(str(value))
    claim = packet.get("claimability") if isinstance(packet.get("claimability"), dict) else {}
    for field in ("proof_strength", "evidence_role"):
        if claim.get(field):
            roles.add(str(claim[field]))
    raw = packet.get("raw") if isinstance(packet.get("raw"), dict) else {}
    for item in raw.get("source_navigation_evidence", []) if isinstance(raw.get("source_navigation_evidence"), list) else []:
        if isinstance(item, dict):
            roles.add(str(item.get("proof_strength") or "source_navigation_evidence"))
    return {role for role in roles if role}


def _output_lines(packet: dict[str, Any]) -> int:
    lines = 0
    for key in ("snippets", "text_evidence"):
        values = packet.get(key, [])
        if not isinstance(values, list):
            continue
        for item in values:
            if isinstance(item, dict):
                lines += max(1, len(str(item.get("text") or "").splitlines()))
            elif item:
                lines += max(1, len(str(item).splitlines()))
    return lines


def _flood_events(packet: dict[str, Any]) -> int:
    raw = packet.get("raw") if isinstance(packet.get("raw"), dict) else {}
    flood = raw.get("flood_diagnostics") if isinstance(raw.get("flood_diagnostics"), dict) else {}
    events = flood.get("events") if isinstance(flood.get("events"), list) else []
    risks = packet.get("risks") if isinstance(packet.get("risks"), list) else []
    return len(events) + sum(1 for risk in risks if isinstance(risk, dict) and risk.get("kind") == "rg_flood")


def _useful_file_diversity(files: list[str]) -> float | None:
    if not files:
        return None
    unique_dirs = {"/".join(normalize_path(path).split("/")[:-1]) or "." for path in files[:10]}
    return len(unique_dirs) / min(10, len(files))


def _evidence_alignment(task: dict[str, Any], files: list[str], symbols: list[str]) -> dict[str, Any]:
    gold_files = {normalize_path(value) for value in task.get("gold_files", [])}
    gold_symbols = {str(value) for value in task.get("gold_symbols", [])}
    returned_files = {normalize_path(value) for value in files}
    returned_symbols = {str(value) for value in symbols}
    return {
        "gold_files_missing_from_context": sorted(gold_files - returned_files),
        "gold_symbols_missing_from_context": sorted(gold_symbols - returned_symbols),
        "all_gold_files_present": bool(gold_files) and gold_files.issubset(returned_files) if gold_files else None,
        "all_gold_symbols_present": bool(gold_symbols) and gold_symbols.issubset(returned_symbols) if gold_symbols else None,
    }


def _routing_packet_usefulness(task: dict[str, Any], packet: dict[str, Any]) -> float | None:
    family = str(task.get("task_family") or task.get("task_type") or "").lower()
    if "routing" not in family and "large_codebase" not in family:
        return None
    raw = packet.get("raw") if isinstance(packet.get("raw"), dict) else {}
    fields = [
        bool(raw.get("routing_packet_fields_used")),
        bool(raw.get("task_intent")),
        bool(raw.get("retrieval_plan_summary")),
        bool(packet.get("follow_up_queries")),
        bool(packet.get("validation_steps")),
    ]
    return sum(1 for value in fields if value) / len(fields)


def _implementation_trace_usefulness(task: dict[str, Any], packet: dict[str, Any], files: list[str], symbols: list[str]) -> float | None:
    family = str(task.get("task_family") or task.get("task_type") or "").lower()
    if "implementation_trace" not in family and "debug" not in family and "dataflow" not in family:
        return None
    raw = packet.get("raw") if isinstance(packet.get("raw"), dict) else {}
    signals = [
        bool(set(normalize_path(value) for value in task.get("gold_files", [])) & set(files)),
        bool(set(str(value) for value in task.get("gold_symbols", [])) & set(symbols)) if task.get("gold_symbols") else True,
        bool(raw.get("source_navigation_evidence")) or any(
            isinstance(item, dict) and item.get("proof_strength") == "source_navigation_evidence"
            for item in packet.get("text_evidence", [])
        ),
        _artifact_db_requirement_correct(task, packet) is not False,
    ]
    return sum(1 for value in signals if value) / len(signals)


def _artifact_db_requirement_correct(task: dict[str, Any], packet: dict[str, Any]) -> bool | None:
    expected = bool(task.get("artifact_db_inspection_required"))
    family = str(task.get("task_family") or task.get("task_type") or "").lower()
    if not expected and "implementation_trace" not in family:
        return None
    raw = packet.get("raw") if isinstance(packet.get("raw"), dict) else {}
    has_requirement = bool(raw.get("artifact_db_inspection_requirements")) or any(
        isinstance(item, dict)
        and (
            "artifact" in str(item.get("kind") or item.get("description") or "").lower()
            or "db" in str(item.get("kind") or item.get("description") or "").lower()
        )
        for item in packet.get("validation_steps", [])
    )
    return has_requirement if expected or "implementation_trace" in family else None


def _raw_timing(raw: dict[str, Any], key: str) -> int:
    timing = raw.get("codegraph_subprocess_timing_ms") if isinstance(raw.get("codegraph_subprocess_timing_ms"), dict) else {}
    try:
        return int(timing.get(key) or 0)
    except (TypeError, ValueError):
        return 0


def _poison_metrics(
    task: dict[str, Any],
    packet: dict[str, Any],
    files: list[str],
    symbols: list[str],
) -> dict[str, Any]:
    forbidden_files = [normalize_path(str(item)) for item in task.get("forbidden_files", [])]
    forbidden_symbols = [str(item) for item in task.get("forbidden_symbols", [])]
    if not forbidden_files and not forbidden_symbols:
        return {
            "forbidden_file_hits_at_5": None,
            "forbidden_file_hits_at_10": None,
            "forbidden_symbol_hits_at_5": None,
            "forbidden_symbol_hits_at_10": None,
            "context_poison_count": None,
            "context_poison_rate": None,
            "first_forbidden_rank": None,
            "forbidden_context_bytes": None,
            "dangerous_context_present": None,
        }
    forbidden_file_set = set(forbidden_files)
    forbidden_symbol_set = set(forbidden_symbols)
    file_hit_ranks = [
        index
        for index, value in enumerate(files, 1)
        if normalize_path(value) in forbidden_file_set
    ]
    symbol_hit_ranks = [
        index
        for index, value in enumerate(symbols, 1)
        if str(value) in forbidden_symbol_set
    ]
    poison_count = len(file_hit_ranks) + len(symbol_hit_ranks)
    returned_count = len(files) + len(symbols)
    first_forbidden = min(file_hit_ranks + symbol_hit_ranks) if file_hit_ranks or symbol_hit_ranks else None
    return {
        "forbidden_file_hits_at_5": sum(1 for rank in file_hit_ranks if rank <= 5),
        "forbidden_file_hits_at_10": sum(1 for rank in file_hit_ranks if rank <= 10),
        "forbidden_symbol_hits_at_5": sum(1 for rank in symbol_hit_ranks if rank <= 5),
        "forbidden_symbol_hits_at_10": sum(1 for rank in symbol_hit_ranks if rank <= 10),
        "context_poison_count": poison_count,
        "context_poison_rate": _divide(poison_count, returned_count),
        "first_forbidden_rank": first_forbidden,
        "forbidden_context_bytes": _forbidden_context_bytes(packet, forbidden_file_set),
        "dangerous_context_present": poison_count > 0,
    }


def _forbidden_context_bytes(packet: dict[str, Any], forbidden_files: set[str]) -> int | None:
    if not forbidden_files:
        return None
    total = 0
    saw_snippet = False
    for snippet in packet.get("snippets", []):
        if not isinstance(snippet, dict):
            continue
        file_path = _file_value(snippet)
        if file_path and normalize_path(file_path) in forbidden_files:
            saw_snippet = True
            total += len(str(snippet.get("text", "")).encode("utf-8", errors="ignore"))
    return total if saw_snippet else 0


def _divide(numerator: float | int | None, denominator: float | int | None) -> float | None:
    if numerator is None or denominator in (None, 0):
        return None
    return float(numerator) / float(denominator)
