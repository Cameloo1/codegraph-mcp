from __future__ import annotations

import re
from pathlib import Path

IDENTIFIER = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]{3,}\b")


def added_identifiers_from_patch(patch_text: str) -> set[str]:
    identifiers: set[str] = set()
    for line in patch_text.splitlines():
        if line.startswith("+") and not line.startswith("+++"):
            identifiers.update(IDENTIFIER.findall(line))
    return identifiers


def nonexistent_symbol_refs(patch_text: str, repo_root: Path | None = None, known_symbols: set[str] | None = None) -> int | None:
    if known_symbols is None:
        return None
    return len(added_identifiers_from_patch(patch_text) - known_symbols)


def unsupported_claim_violations(packet: dict) -> int:
    violations = 0
    for item in _walk(packet):
        if not isinstance(item, dict):
            continue
        graph_proof = item.get("graph_proof")
        evidence_role = str(item.get("proof_strength") or item.get("evidence_type") or "")
        candidate_source = str(item.get("candidate_source") or "")
        if graph_proof is True and evidence_role in {"text_evidence", "candidate_evidence", "source_navigation_evidence"}:
            violations += 1
        if graph_proof is True and candidate_source in {"vector_semantic", "binary_vector", "nuance_rescue", "lexical"}:
            violations += 1
    return violations


def _walk(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk(child)
