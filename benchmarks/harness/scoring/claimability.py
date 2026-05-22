from __future__ import annotations

from typing import Any

GRAPH_PROOF_STRENGTHS = {"graph_relation_proof", "mutation_proof", "flow_proof"}
NON_PROOF_STRENGTHS = {"text_evidence", "symbol_evidence", "candidate_evidence", "source_navigation_evidence"}


def claimability_violations(packet: dict[str, Any]) -> list[str]:
    violations: list[str] = []
    for item in _walk(packet):
        if not isinstance(item, dict):
            continue
        graph_proof = item.get("graph_proof")
        proof_strength = str(item.get("proof_strength") or item.get("evidence_type") or "")
        proof_status = str(item.get("proof_status") or "")
        if graph_proof is True and proof_strength in NON_PROOF_STRENGTHS:
            violations.append(f"{proof_strength} marked graph_proof=true")
        if (
            graph_proof is True
            and proof_strength
            and proof_status not in {"proof_path_found", "verified"}
            and proof_strength not in GRAPH_PROOF_STRENGTHS
        ):
            violations.append(f"unsupported proof_strength marked graph_proof=true: {proof_strength}")
        if proof_status in {"stale", "diagnostic_only"} and item.get("claimable") is True:
            violations.append(f"{proof_status} item marked claimable")
    if packet.get("claimability", {}).get("graph_proof") is True:
        strength = str(packet.get("claimability", {}).get("proof_strength") or "")
        if strength in NON_PROOF_STRENGTHS:
            violations.append(f"packet-level {strength} marked graph_proof=true")
    return violations


def no_proof_behavior_ok(task: dict[str, Any], packet: dict[str, Any]) -> bool | None:
    expected = task.get("expected_claimability", {})
    expected_status = expected.get("expected_proof_status")
    if not expected_status:
        return None
    claimability = packet.get("claimability", {})
    if expected_status == "no_proof_path_found":
        return claimability.get("graph_proof") is not True
    return True


def _walk(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk(child)
