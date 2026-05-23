from __future__ import annotations

from typing import Any

from benchmarks.harness.scoring.context_recall import normalize_path
from benchmarks.harness.scoring.hallucination import unsupported_claim_violations


PATCH_OUTCOME_PLACEHOLDER_METRICS = {
    "resolved_percentage",
    "test_pass_rate",
    "patch_size",
    "retry_count",
    "cost",
    "wrong_file_edits",
    "nonexistent_symbol_references",
    "evidence_alignment",
}

OVERCLAIM_PROOF_SOURCES = {
    "text_evidence",
    "candidate_evidence",
    "source_navigation_evidence",
    "vector_semantic",
    "binary_vector",
    "nuance_rescue",
    "lexical",
}

UNSAFE_CONFIDENCE = {"certain", "guaranteed", "proven", "definitive"}


def routing_packet_quality(task: dict[str, Any], packet: dict[str, Any]) -> dict[str, Any]:
    components = [
        _exact_component(
            "task_intent",
            task.get("expected_intent"),
            _first_value(packet, "task_intent", "intent", "expected_intent"),
        ),
        _coverage_component(
            "critical_file_recall",
            task.get("expected_critical_files", []),
            _list_values(packet, "critical_files", "files", "planned_files"),
            path_like=True,
        ),
        _coverage_component(
            "critical_symbol_recall",
            task.get("expected_critical_symbols", []),
            _list_values(packet, "critical_symbols", "symbols", "planned_symbols"),
        ),
        _coverage_component(
            "role_coverage",
            task.get("expected_proof_ladder", []),
            _proof_ladder_values(packet),
        ),
        _exact_component(
            "proof_status",
            task.get("expected_proof_status"),
            _first_value(packet, "proof_status", "expected_proof_status"),
        ),
        _coverage_component("risks_coverage", task.get("expected_risks", []), _list_values(packet, "risks")),
        _coverage_component("unknowns_coverage", task.get("expected_unknowns", []), _list_values(packet, "unknowns")),
        _coverage_component(
            "validation_steps_coverage",
            task.get("expected_validation_steps", []),
            _list_values(packet, "validation_steps"),
        ),
        _presence_component("follow_up_queries", _list_values(packet, "follow_up_queries")),
        _presence_component("evidence_backed_edit_plan", _evidence_backed_edit_plan(packet)),
        _forbidden_absence_component(
            "forbidden_file_hits",
            task.get("forbidden_files", []) + task.get("forbidden_edit_targets", []),
            _list_values(packet, "critical_files", "files", "planned_files", "edit_targets"),
            path_like=True,
        ),
        _forbidden_absence_component(
            "forbidden_symbol_hits",
            task.get("forbidden_symbols", []),
            _list_values(packet, "critical_symbols", "symbols", "planned_symbols"),
        ),
        _unsupported_claim_component(packet),
    ]
    return _result("routing_packet_quality", components)


def plan_accuracy(task: dict[str, Any], plan: dict[str, Any]) -> dict[str, Any]:
    edit_targets = _list_values(plan, "edit_targets", "implementation_surface", "files")
    components = [
        _coverage_component(
            "correct_implementation_surface",
            task.get("expected_critical_files", []),
            edit_targets,
            path_like=True,
        ),
        _coverage_component("correct_tests", task.get("expected_tests", []), _list_values(plan, "tests", "expected_tests")),
        _forbidden_absence_component(
            "no_nonexistent_or_forbidden_symbols",
            task.get("forbidden_symbols", []),
            _list_values(plan, "symbols", "symbol_references", "nonexistent_symbols"),
        ),
        _forbidden_absence_component(
            "no_wrong_files",
            task.get("forbidden_files", []) + task.get("forbidden_edit_targets", []),
            edit_targets,
            path_like=True,
        ),
        _coverage_component("correct_unknowns", task.get("expected_unknowns", []), _list_values(plan, "unknowns", "assumptions")),
        _coverage_component(
            "correct_validation_path",
            task.get("expected_validation_steps", []),
            _list_values(plan, "validation_steps"),
        ),
        _over_editing_component(task, edit_targets),
        _coverage_component(
            "architecture_explanation_coverage",
            task.get("expected_plan_facts", []),
            _list_values(plan, "plan_facts", "architecture_notes", "explanation_facts"),
        ),
        _forbidden_claim_component(task, plan),
    ]
    return _result("plan_accuracy", components)


def proof_discipline(task: dict[str, Any], output: dict[str, Any]) -> dict[str, Any]:
    expected_status = task.get("expected_proof_status")
    components = [
        _exact_component("proof_status", expected_status, _first_value(output, "proof_status", "expected_proof_status")),
        _coverage_component("proof_ladder", task.get("expected_proof_ladder", []), _proof_ladder_values(output)),
        _unsupported_claim_component(output),
        _forbidden_claim_component(task, output),
        _coverage_component("unknowns_present", task.get("expected_unknowns", []), _list_values(output, "unknowns")),
        _no_proof_component(expected_status, output),
    ]
    return _result("proof_discipline", components)


def hallucination_trap(task: dict[str, Any], output: dict[str, Any]) -> dict[str, Any]:
    edit_targets = _list_values(output, "edit_targets", "files", "changed_files")
    symbols = _list_values(output, "symbols", "symbol_references", "nonexistent_symbols")
    components = [
        _forbidden_absence_component(
            "wrong_file_edits",
            task.get("forbidden_files", []) + task.get("forbidden_edit_targets", []),
            edit_targets,
            path_like=True,
        ),
        _forbidden_absence_component("nonexistent_symbol_references", task.get("forbidden_symbols", []), symbols),
        _forbidden_claim_component(task, output),
        _coverage_component(
            "valid_validation_steps",
            task.get("expected_validation_steps", []),
            _list_values(output, "validation_steps"),
        ),
        _unsafe_confidence_component(task, output),
        _coverage_component("ambiguity_acknowledgement", _ambiguity_expectations(task), _ambiguity_observations(output)),
        _stale_foreign_evidence_component(output),
        _benchmark_leakage_component(output),
    ]
    return _result("hallucination_trap", components)


def patch_outcome_placeholder(
    task: dict[str, Any],
    patch_result: dict[str, Any] | None = None,
    *,
    enabled: bool = False,
    external_agent_harness_configured: bool = False,
) -> dict[str, Any]:
    metrics = {metric: "not_applicable" for metric in sorted(PATCH_OUTCOME_PLACEHOLDER_METRICS)}
    if not enabled:
        return {
            "scorer": "patch_outcome_placeholder",
            "status": "disabled",
            "enabled": False,
            "score": None,
            "max_score": None,
            "normalized_score": None,
            "metrics": metrics,
            "reasons": ["patch outcome scoring is disabled by default for v0.5 transition tasks"],
        }
    if not external_agent_harness_configured:
        return {
            "scorer": "patch_outcome_placeholder",
            "status": "blocked_external",
            "enabled": False,
            "score": None,
            "max_score": None,
            "normalized_score": None,
            "metrics": metrics,
            "reasons": ["real external-agent patch-quality harness is not configured"],
        }
    return {
        "scorer": "patch_outcome_placeholder",
        "status": "blocked_external",
        "enabled": False,
        "score": None,
        "max_score": None,
        "normalized_score": None,
        "metrics": metrics,
        "reasons": ["activation requires a later explicit v1 patch-quality benchmark prompt"],
        "input_seen": bool(task or patch_result),
    }


def _result(scorer: str, components: list[dict[str, Any]]) -> dict[str, Any]:
    score = sum(component["score"] for component in components)
    max_score = sum(component["max_score"] for component in components)
    reasons = [
        reason
        for component in components
        for reason in component.get("reasons", [])
    ]
    normalized = score / max_score if max_score else None
    return {
        "scorer": scorer,
        "status": "pass" if normalized == 1.0 else "fail",
        "score": score,
        "max_score": max_score,
        "normalized_score": normalized,
        "components": components,
        "reasons": reasons,
    }


def _exact_component(name: str, expected: Any, observed: Any) -> dict[str, Any]:
    if expected in (None, "", []):
        return _component(name, 0.0, 0.0, "not_applicable", [f"{name}: no expectation configured"])
    if observed in (None, "", []):
        return _component(name, 0.0, 1.0, "missing", [f"{name}: missing observed value"])
    if _key(expected) == _key(observed):
        return _component(name, 1.0, 1.0, "pass", [])
    return _component(name, 0.0, 1.0, "fail", [f"{name}: expected {expected!r}, observed {observed!r}"])


def _coverage_component(
    name: str,
    expected_values: list[Any],
    observed_values: list[Any],
    *,
    path_like: bool = False,
) -> dict[str, Any]:
    expected = [_normalize(item, path_like=path_like) for item in expected_values if str(item).strip()]
    observed = [_normalize(item, path_like=path_like) for item in observed_values if str(item).strip()]
    if not expected:
        return _component(name, 0.0, 0.0, "not_applicable", [f"{name}: no expectation configured"])
    if not observed:
        return _component(name, 0.0, 1.0, "missing", [f"{name}: missing observed values"])
    expected_set = set(expected)
    observed_set = set(observed)
    missing = sorted(expected_set - observed_set)
    score = (len(expected_set) - len(missing)) / len(expected_set)
    status = "pass" if not missing else "fail"
    reasons = [f"{name}: missing {missing}"] if missing else []
    return _component(name, score, 1.0, status, reasons)


def _presence_component(name: str, observed_values: list[Any] | bool) -> dict[str, Any]:
    present = bool(observed_values)
    return _component(name, 1.0 if present else 0.0, 1.0, "pass" if present else "missing", [] if present else [f"{name}: missing"])


def _forbidden_absence_component(
    name: str,
    forbidden_values: list[Any],
    observed_values: list[Any],
    *,
    path_like: bool = False,
) -> dict[str, Any]:
    forbidden = {_normalize(item, path_like=path_like) for item in forbidden_values if str(item).strip()}
    if not forbidden:
        return _component(name, 0.0, 0.0, "not_applicable", [f"{name}: no forbidden values configured"])
    observed = {_normalize(item, path_like=path_like) for item in observed_values if str(item).strip()}
    hits = sorted(forbidden & observed)
    if hits:
        return _component(name, 0.0, 1.0, "fail", [f"{name}: forbidden hits {hits}"])
    return _component(name, 1.0, 1.0, "pass", [])


def _unsupported_claim_component(output: dict[str, Any]) -> dict[str, Any]:
    violations = unsupported_claim_violations(output) + _overclaim_source_count(output)
    if violations:
        return _component("unsupported_proof_claims", 0.0, 1.0, "fail", [f"unsupported_proof_claims: {violations} violation(s)"])
    return _component("unsupported_proof_claims", 1.0, 1.0, "pass", [])


def _forbidden_claim_component(task: dict[str, Any], output: dict[str, Any]) -> dict[str, Any]:
    forbidden_claims = [str(value) for value in task.get("forbidden_plan_claims", []) if str(value).strip()]
    if not forbidden_claims:
        return _component("forbidden_plan_claims", 0.0, 0.0, "not_applicable", ["forbidden_plan_claims: no forbidden claims configured"])
    claim_text = " ".join(str(value) for value in _list_values(output, "claims", "plan_claims", "relation_claims", "notes"))
    hits = [claim for claim in forbidden_claims if _key(claim) in _key(claim_text)]
    if hits:
        return _component("forbidden_plan_claims", 0.0, 1.0, "fail", [f"forbidden_plan_claims: {hits}"])
    return _component("forbidden_plan_claims", 1.0, 1.0, "pass", [])


def _over_editing_component(task: dict[str, Any], edit_targets: list[Any]) -> dict[str, Any]:
    expected = {_normalize(item, path_like=True) for item in task.get("expected_critical_files", [])}
    if not expected:
        return _component("over_editing_risk", 0.0, 0.0, "not_applicable", ["over_editing_risk: no expected files configured"])
    observed = {_normalize(item, path_like=True) for item in edit_targets if str(item).strip()}
    extra = sorted(observed - expected)
    if extra:
        return _component("over_editing_risk", 0.0, 1.0, "fail", [f"over_editing_risk: unexpected edit targets {extra}"])
    return _component("over_editing_risk", 1.0, 1.0, "pass", [])


def _no_proof_component(expected_status: Any, output: dict[str, Any]) -> dict[str, Any]:
    if expected_status != "no_proof_path_found":
        return _component("correct_no_proof_behavior", 0.0, 0.0, "not_applicable", ["correct_no_proof_behavior: no no-proof expectation"])
    proof_status = _first_value(output, "proof_status", "expected_proof_status")
    graph_proof = bool(_first_value(output, "graph_proof", "claims_graph_proof"))
    if proof_status == "no_proof_path_found" and not graph_proof:
        return _component("correct_no_proof_behavior", 1.0, 1.0, "pass", [])
    return _component("correct_no_proof_behavior", 0.0, 1.0, "fail", ["correct_no_proof_behavior: graph proof overclaimed or no-proof status missing"])


def _unsafe_confidence_component(task: dict[str, Any], output: dict[str, Any]) -> dict[str, Any]:
    confidence = _key(_first_value(output, "confidence", "certainty"))
    expected_unknowns = [value for value in task.get("expected_unknowns", []) if str(value).strip()]
    observed_unknowns = _list_values(output, "unknowns")
    if confidence in UNSAFE_CONFIDENCE and (expected_unknowns or task.get("expected_proof_status") == "no_proof_path_found"):
        return _component("unsafe_confidence", 0.0, 1.0, "fail", [f"unsafe_confidence: {confidence}"])
    if expected_unknowns and not observed_unknowns:
        return _component("unsafe_confidence", 0.0, 1.0, "missing", ["unsafe_confidence: expected unknowns missing"])
    return _component("unsafe_confidence", 1.0, 1.0, "pass", [])


def _stale_foreign_evidence_component(output: dict[str, Any]) -> dict[str, Any]:
    stale = bool(output.get("stale_evidence_used") or output.get("foreign_evidence_used"))
    evidence_state = _key(output.get("evidence_state"))
    if stale or evidence_state in {"stale", "foreign", "stale_docs_only"}:
        return _component("stale_or_foreign_evidence_misuse", 0.0, 1.0, "fail", ["stale_or_foreign_evidence_misuse: stale or foreign evidence used"])
    return _component("stale_or_foreign_evidence_misuse", 1.0, 1.0, "pass", [])


def _benchmark_leakage_component(output: dict[str, Any]) -> dict[str, Any]:
    if output.get("uses_hidden_gold_terms") or output.get("benchmark_query_leakage"):
        return _component("benchmark_query_leakage", 0.0, 1.0, "fail", ["benchmark_query_leakage: hidden evaluator terms used"])
    return _component("benchmark_query_leakage", 1.0, 1.0, "pass", [])


def _ambiguity_expectations(task: dict[str, Any]) -> list[str]:
    values = task.get("expected_unknowns", []) + task.get("expected_risks", [])
    return [value for value in values if "ambig" in _key(value) or "same" in _key(value)]


def _ambiguity_observations(output: dict[str, Any]) -> list[str]:
    values = _list_values(output, "unknowns", "risks", "ambiguities_acknowledged")
    return [value for value in values if "ambig" in _key(value) or "same" in _key(value)]


def _evidence_backed_edit_plan(packet: dict[str, Any]) -> bool:
    plans = packet.get("edit_plan") or packet.get("planned_edits") or []
    if not isinstance(plans, list):
        return False
    for item in plans:
        if isinstance(item, dict) and (item.get("evidence") or item.get("evidence_ref") or item.get("proof_status")):
            return True
    return False


def _proof_ladder_values(output: dict[str, Any]) -> list[Any]:
    values = _list_values(output, "proof_ladder", "evidence_labels", "roles")
    for item in _list_values(output, "evidence", "text_evidence", "snippets"):
        if isinstance(item, dict):
            values.extend(_list_values(item, "label", "proof_strength", "evidence_type", "role"))
    return values


def _overclaim_source_count(output: dict[str, Any]) -> int:
    violations = 0
    for item in _walk(output):
        if not isinstance(item, dict):
            continue
        if item.get("graph_proof") is not True:
            continue
        evidence_values = {
            _key(item.get("proof_strength")),
            _key(item.get("evidence_type")),
            _key(item.get("candidate_source")),
            _key(item.get("source")),
        }
        if evidence_values & OVERCLAIM_PROOF_SOURCES:
            violations += 1
    return violations


def _first_value(mapping: dict[str, Any], *keys: str) -> Any:
    for key in keys:
        value = mapping.get(key)
        if value not in (None, "", []):
            return value
    return None


def _list_values(mapping: dict[str, Any], *keys: str) -> list[Any]:
    values: list[Any] = []
    for key in keys:
        value = mapping.get(key)
        if value is None:
            continue
        if isinstance(value, list):
            values.extend(_flatten_value(item) for item in value)
        elif isinstance(value, dict):
            values.extend(_flatten_value(item) for item in value.values())
        else:
            values.append(value)
    return [value for value in values if value not in (None, "")]


def _flatten_value(value: Any) -> Any:
    if isinstance(value, dict):
        for key in ("file", "path", "file_path", "symbol", "name", "text", "description", "label", "role"):
            if value.get(key):
                return value[key]
    return value


def _normalize(value: Any, *, path_like: bool = False) -> str:
    text = str(value)
    if path_like:
        text = normalize_path(text)
    return _key(text)


def _key(value: Any) -> str:
    return str(value or "").replace("\\", "/").strip().lower()


def _component(name: str, score: float, max_score: float, status: str, reasons: list[str]) -> dict[str, Any]:
    return {
        "name": name,
        "score": score,
        "max_score": max_score,
        "status": status,
        "reasons": reasons,
    }


def _walk(value: Any):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk(child)
