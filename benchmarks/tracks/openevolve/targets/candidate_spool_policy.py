"""Standalone candidate-spool packet policy for OpenEvolve tuning.

This file is intentionally pure Python and isolated from CodeGraph product code.
OpenEvolve may mutate this file during experimental runs; any useful idea still
needs a separate human-reviewed Rust implementation pass.
"""

from __future__ import annotations

import json
import re
from collections import defaultdict
from typing import Any, Dict, Iterable, List, Mapping, MutableMapping


MAX_PACKET_BYTES = 7000
MAX_PACKETS = 8
MAX_CANDIDATES_PER_FILE = 4
MAX_CANDIDATES_PER_KIND = 14
MAX_CANDIDATES_PER_SOURCE = 16


def _tokens(value: Any) -> set[str]:
    if value is None:
        return set()
    if isinstance(value, (list, tuple, set)):
        text = " ".join(str(item) for item in value)
    else:
        text = str(value)
    return {token.lower() for token in re.findall(r"[A-Za-z0-9_./+-]+", text)}


def _candidate_text(candidate: Mapping[str, Any]) -> str:
    parts: List[str] = [
        str(candidate.get("path", "")),
        str(candidate.get("filename", "")),
        str(candidate.get("candidate_kind", "")),
        str(candidate.get("source_kind", "")),
        str(candidate.get("evidence_role", "")),
        str(candidate.get("source_role", "")),
        str(candidate.get("text", "")),
    ]
    parts.extend(str(symbol) for symbol in candidate.get("symbols", []) or [])
    return " ".join(parts)


def _candidate_terms(candidate: Mapping[str, Any]) -> set[str]:
    return _tokens(_candidate_text(candidate))


def _is_unsafe_claim(candidate: Mapping[str, Any]) -> bool:
    claimability = candidate.get("claimability") or {}
    return bool(
        candidate.get("graph_proof")
        or candidate.get("claimable_graph")
        or claimability.get("graph_proof")
        or claimability.get("claimable_graph")
    )


def score_candidate(candidate: Mapping[str, Any], query: Mapping[str, Any]) -> float:
    """Score one candidate for a query. Higher is better."""
    if _is_unsafe_claim(candidate):
        return -1000.0

    query_terms = _tokens(query.get("terms", []))
    path_terms = _tokens(query.get("path_terms", []))
    symbol_terms = _tokens(query.get("symbol_terms", []))
    candidate_terms = _candidate_terms(candidate)

    path = str(candidate.get("path", "")).lower()
    filename = str(candidate.get("filename", "")).lower()
    symbols = {str(symbol).lower() for symbol in candidate.get("symbols", []) or []}
    text = str(candidate.get("text", "")).lower()

    overlap = len(query_terms & candidate_terms)
    path_overlap = sum(1 for term in path_terms if term in path or term in filename)
    symbol_overlap = len(symbol_terms & symbols)
    text_overlap = sum(1 for term in query_terms if term and term in text)

    role_bonus = 0.0
    if candidate.get("evidence_role") in {"path", "symbol", "source_navigation"}:
        role_bonus += 0.35
    if candidate.get("candidate_kind") in {"file_path", "symbol_signature"}:
        role_bonus += 0.25
    if candidate.get("source_role") in {"definition", "config", "build_rule", "test"}:
        role_bonus += 0.15

    hint = float(candidate.get("score_hint", 0.0) or 0.0)
    return (
        1.15 * overlap
        + 2.0 * path_overlap
        + 2.35 * symbol_overlap
        + 0.55 * text_overlap
        + role_bonus
        + hint
    )


def packet_key(candidate: Mapping[str, Any]) -> str:
    """Group related candidates into auditable packets."""
    path = str(candidate.get("path") or "unknown")
    role = str(candidate.get("source_role") or candidate.get("evidence_role") or "candidate")
    return f"{path}::{role}"


def _candidate_size(candidate: Mapping[str, Any]) -> int:
    if isinstance(candidate.get("bytes_estimate"), int):
        return max(1, int(candidate["bytes_estimate"]))
    return len(json.dumps(candidate, sort_keys=True, separators=(",", ":")).encode("utf-8"))


def should_keep_candidate(candidate: Mapping[str, Any], state: MutableMapping[str, Any]) -> bool:
    """Apply deterministic caps before packet creation."""
    if _is_unsafe_claim(candidate):
        return False

    path = str(candidate.get("path") or "unknown")
    kind = str(candidate.get("candidate_kind") or "unknown")
    source = str(candidate.get("source_kind") or "unknown")
    size = _candidate_size(candidate)

    if state["total_bytes"] + size > state["max_total_bytes"]:
        return False
    if state["by_file"][path] >= MAX_CANDIDATES_PER_FILE:
        return False
    if state["by_kind"][kind] >= MAX_CANDIDATES_PER_KIND:
        return False
    if state["by_source"][source] >= MAX_CANDIDATES_PER_SOURCE:
        return False

    state["total_bytes"] += size
    state["by_file"][path] += 1
    state["by_kind"][kind] += 1
    state["by_source"][source] += 1
    return True


def _make_packet(key: str, members: List[Mapping[str, Any]], query: Mapping[str, Any]) -> Dict[str, Any]:
    best = max(members, key=lambda item: score_candidate(item, query))
    symbols: List[str] = []
    for member in members:
        for symbol in member.get("symbols", []) or []:
            if symbol not in symbols:
                symbols.append(str(symbol))

    snippets = []
    for member in members[:3]:
        text = str(member.get("text", "")).strip()
        if text:
            snippets.append(text[:240])

    return {
        "packet_key": key,
        "path": best.get("path"),
        "filename": best.get("filename"),
        "candidate_kind": best.get("candidate_kind"),
        "source_kind": best.get("source_kind"),
        "evidence_kind": "candidate_evidence",
        "graph_proof": False,
        "claimable_graph": False,
        "score": score_candidate(best, query),
        "symbols": symbols[:12],
        "snippets": snippets,
        "member_count": len(members),
    }


def rank_packet(packet: Mapping[str, Any], query: Mapping[str, Any]) -> float:
    """Rank packet output for an agent-facing context request."""
    query_terms = _tokens(query.get("terms", []))
    symbol_terms = _tokens(query.get("symbol_terms", []))
    path_terms = _tokens(query.get("path_terms", []))

    path = str(packet.get("path", "")).lower()
    symbols = {str(symbol).lower() for symbol in packet.get("symbols", []) or []}
    snippets = " ".join(str(snippet).lower() for snippet in packet.get("snippets", []) or [])

    return float(packet.get("score", 0.0) or 0.0) + (
        2.0 * sum(1 for term in path_terms if term in path)
        + 2.5 * len(symbol_terms & symbols)
        + 0.35 * sum(1 for term in query_terms if term in snippets)
        + min(int(packet.get("member_count", 1) or 1), 4) * 0.05
    )


def select_packets(
    candidates: Iterable[Mapping[str, Any]],
    query: Mapping[str, Any],
    budget: Mapping[str, Any],
) -> List[Dict[str, Any]]:
    """Select bounded, claim-safe candidate packets for one query."""
    max_packets = int(budget.get("max_packets", MAX_PACKETS))
    max_total_bytes = int(budget.get("max_packet_bytes", MAX_PACKET_BYTES))

    scored = sorted(
        ((score_candidate(candidate, query), index, candidate) for index, candidate in enumerate(candidates)),
        key=lambda item: (-item[0], item[1]),
    )
    state: Dict[str, Any] = {
        "total_bytes": 0,
        "max_total_bytes": max_total_bytes,
        "by_file": defaultdict(int),
        "by_kind": defaultdict(int),
        "by_source": defaultdict(int),
    }
    grouped: Dict[str, List[Mapping[str, Any]]] = defaultdict(list)

    for score, _index, candidate in scored:
        if score <= -999:
            continue
        if should_keep_candidate(candidate, state):
            grouped[packet_key(candidate)].append(candidate)

    packets = [_make_packet(key, members, query) for key, members in grouped.items()]
    packets.sort(key=lambda packet: (-rank_packet(packet, query), str(packet.get("path", ""))))

    return packets[:max_packets]

