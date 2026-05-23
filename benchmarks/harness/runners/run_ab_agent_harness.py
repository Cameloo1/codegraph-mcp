from __future__ import annotations

import argparse
import json
import os
import time
from pathlib import Path
from typing import Any

from benchmarks.harness.agent_reliability_registry import (
    load_agent_reliability_tasks,
    provider_visible_agent_task,
)
from benchmarks.harness.agent_scaffolds.mock_agent import MockAgent
from benchmarks.harness.scoring.agent_reliability import (
    hallucination_trap,
    patch_outcome_placeholder,
    plan_accuracy,
    proof_discipline,
)
from benchmarks.harness.task_sanitizer import EVALUATOR_ONLY_FIELDS
from benchmarks.harness.workspace import ensure_dir, safe_slug


AB_HARNESS_SCHEMA_VERSION = "ab_agent_harness_run_v1"
AB_PLAN_RESULT_SCHEMA_VERSION = "ab_agent_plan_result_v1"
AB_STATUS_VALUES = {"blocked", "not_configured", "scaffold_only", "diagnostic_only", "completed", "failed"}
AB_HARNESS_MODES = {
    "rg_only_agent_plan",
    "rg_plus_codegraph_agent_plan",
    "rg_only_agent_patch",
    "rg_plus_codegraph_agent_patch",
    "mock_agent_scaffold_only",
    "deterministic_plan_only",
}
PLAN_MODES = {"rg_only_agent_plan", "rg_plus_codegraph_agent_plan", "deterministic_plan_only"}
PATCH_MODES = {"rg_only_agent_patch", "rg_plus_codegraph_agent_patch"}
NORMAL_TOOLS = ("rg", "search", "edit", "test")
CODEGRAPH_TOOL = "codegraph"
DEFAULT_MODEL = "none"
DEFAULT_AGENT_SCAFFOLD = "same_agent_scaffold_v1"
DEFAULT_EVALUATOR = "deterministic_agent_reliability_scorers_v1"
DEFAULT_BUDGET = {
    "timeout_s": 120,
    "max_tool_calls": 12,
    "max_context_bytes": 60000,
    "max_input_tokens": None,
    "max_output_tokens": None,
}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--task-registry", default=None)
    parser.add_argument("--max-tasks", type=int, default=2)
    parser.add_argument("--modes", nargs="*", default=sorted(PLAN_MODES | {"mock_agent_scaffold_only"}))
    parser.add_argument("--enable-patch-modes", action="store_true")
    parser.add_argument("--external-agent-command", default="")
    parser.add_argument("--codegraph-available", action="store_true", default=True)
    parser.add_argument("--codegraph-unavailable", action="store_false", dest="codegraph_available")
    args = parser.parse_args(argv)
    summary = run_ab_harness(
        output_dir=Path(args.output_dir),
        task_registry=Path(args.task_registry) if args.task_registry else None,
        max_tasks=args.max_tasks,
        modes=args.modes,
        enable_patch_modes=args.enable_patch_modes,
        external_agent_command=args.external_agent_command,
        codegraph_available=args.codegraph_available,
    )
    print(json.dumps(summary, indent=2))
    return 0 if summary["status"] == "complete" else 1


def run_ab_harness(
    *,
    output_dir: Path,
    task_registry: Path | None = None,
    max_tasks: int = 2,
    modes: list[str] | None = None,
    enable_patch_modes: bool = False,
    external_agent_command: str = "",
    codegraph_available: bool = True,
    model: str = DEFAULT_MODEL,
    agent_scaffold: str = DEFAULT_AGENT_SCAFFOLD,
    evaluator: str = DEFAULT_EVALUATOR,
    budget: dict[str, Any] | None = None,
) -> dict[str, Any]:
    output_dir = ensure_dir(output_dir)
    per_task_dir = ensure_dir(output_dir / "per_task")
    commands_jsonl = output_dir / "commands.jsonl"
    commands_jsonl.write_text("", encoding="utf-8")
    budget = {**DEFAULT_BUDGET, **(budget or {})}
    requested_modes = list(modes or sorted(PLAN_MODES | {"mock_agent_scaffold_only"}))
    unknown_modes = sorted(set(requested_modes) - AB_HARNESS_MODES)
    tasks = load_agent_reliability_tasks(task_registry) if task_registry else load_agent_reliability_tasks()
    tasks = tasks[:max_tasks] if max_tasks is not None else tasks
    external_command = external_agent_command or os.environ.get("CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND", "")
    before_dot_codegraph = Path(".codegraph").exists()
    run_id = output_dir.name or f"ab_agent_harness_{int(time.time())}"
    run_plan = build_run_plan(
        run_id=run_id,
        output_dir=output_dir,
        tasks=tasks,
        modes=requested_modes,
        model=model,
        agent_scaffold=agent_scaffold,
        evaluator=evaluator,
        budget=budget,
        enable_patch_modes=enable_patch_modes,
        external_agent_command=external_command,
        codegraph_available=codegraph_available,
        unknown_modes=unknown_modes,
    )
    _write_json(output_dir / "run_plan.json", run_plan)

    results: list[dict[str, Any]] = []
    for task in tasks:
        task_dir = ensure_dir(per_task_dir / safe_slug(str(task["task_id"])))
        sanitized_task = provider_visible_agent_task(task)
        for mode in requested_modes:
            if mode not in AB_HARNESS_MODES:
                results.append(_mode_error(task, mode, "unknown mode"))
                continue
            result = run_mode(
                task=task,
                sanitized_task=sanitized_task,
                mode=mode,
                output_dir=task_dir / mode,
                model=model,
                agent_scaffold=agent_scaffold,
                evaluator=evaluator,
                budget=budget,
                enable_patch_modes=enable_patch_modes,
                external_agent_command=external_command,
                codegraph_available=codegraph_available,
            )
            results.append(result)

    after_dot_codegraph = Path(".codegraph").exists()
    normal_dot_codegraph_mutated = before_dot_codegraph != after_dot_codegraph
    invariant = validate_same_agent_invariant(run_plan, results)
    leakage_failures = [
        result
        for result in results
        if not result.get("leakage_audit_pass", True) or not result.get("agent_visible_leakage_pass", True)
    ]
    patch_modes_enabled = any(result.get("mode") in PATCH_MODES and result.get("status") == "completed" for result in results)
    summary = {
        "schema_version": AB_HARNESS_SCHEMA_VERSION,
        "status": "complete" if not unknown_modes and not normal_dot_codegraph_mutated and invariant["pass"] and not leakage_failures else "failed",
        "run_id": run_id,
        "public_claim": False,
        "real_patch_quality_claim": False,
        "normal_dot_codegraph_mutated": normal_dot_codegraph_mutated,
        "output_dir": str(output_dir),
        "run_plan": str(output_dir / "run_plan.json"),
        "commands_jsonl": str(commands_jsonl),
        "same_agent_invariant": invariant,
        "gold_leakage_prevented": not leakage_failures,
        "patch_modes_enabled_by_default": patch_modes_enabled if not enable_patch_modes else False,
        "codegraph_availability": run_plan["codegraph_availability"],
        "mode_counts": _mode_counts(results),
        "results": results,
        "claim_boundary": "local A/B harness scaffold; plan-only diagnostics and scaffold readiness only",
    }
    _write_json(output_dir / "summary.json", summary)
    return summary


def build_run_plan(
    *,
    run_id: str,
    output_dir: Path,
    tasks: list[dict[str, Any]],
    modes: list[str],
    model: str,
    agent_scaffold: str,
    evaluator: str,
    budget: dict[str, Any],
    enable_patch_modes: bool,
    external_agent_command: str,
    codegraph_available: bool,
    unknown_modes: list[str] | None = None,
) -> dict[str, Any]:
    task_entries = []
    for task in tasks:
        sanitized = provider_visible_agent_task(task)
        task_entries.append(
            {
                "task_id": task["task_id"],
                "task_family": sanitized.get("task_family"),
                "visible_task_sha": _stable_json_sha(sanitized),
                "visible_fields": sorted(key for key in sanitized if not key.startswith("leakage_audit")),
                "leakage_audit_pass": bool(sanitized.get("leakage_audit", {}).get("pass")),
            }
        )
    return {
        "schema_version": AB_HARNESS_SCHEMA_VERSION,
        "run_id": run_id,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "output_dir": str(output_dir),
        "public_claim": False,
        "real_patch_quality_claim": False,
        "claim_boundary": "scaffold and plan-only diagnostics; no public benchmark claim",
        "comparison": {
            "arm_a": "same agent + normal rg/search/edit/test tools",
            "arm_b": "same agent + normal rg/search/edit/test tools + CodeGraph",
            "only_allowed_difference": "CodeGraph context/routing/proof packet availability in arm B",
        },
        "fixed_metadata": {
            "model": model,
            "agent_scaffold": agent_scaffold,
            "evaluator": evaluator,
            "budget": budget,
            "repo_commit": _git_commit_or_unknown(),
            "environment": {
                "cwd": str(Path.cwd()),
                "external_agent_command": "configured" if external_agent_command else "not_configured",
            },
        },
        "arms": {
            "A": _arm_config("A", uses_codegraph=False, model=model, agent_scaffold=agent_scaffold, evaluator=evaluator, budget=budget),
            "B": _arm_config("B", uses_codegraph=True, model=model, agent_scaffold=agent_scaffold, evaluator=evaluator, budget=budget),
        },
        "modes": [
            _mode_plan(mode, enable_patch_modes=enable_patch_modes, external_agent_command=external_agent_command, codegraph_available=codegraph_available)
            for mode in modes
        ],
        "unknown_modes": sorted(unknown_modes or []),
        "task_registry": "benchmarks/datasets/agent_reliability/agent_reliability_tasks.jsonl",
        "tasks": task_entries,
        "codegraph_availability": {
            "available": codegraph_available,
            "recorded_separately_from_agent_quality": True,
            "status": "completed" if codegraph_available else "blocked",
        },
        "patch_modes_enabled": bool(enable_patch_modes and external_agent_command),
        "patch_modes_enabled_by_default": False,
        "normal_dot_codegraph_before_run": Path(".codegraph").exists(),
        "commands": [],
    }


def run_mode(
    *,
    task: dict[str, Any],
    sanitized_task: dict[str, Any],
    mode: str,
    output_dir: Path,
    model: str,
    agent_scaffold: str,
    evaluator: str,
    budget: dict[str, Any],
    enable_patch_modes: bool,
    external_agent_command: str,
    codegraph_available: bool,
) -> dict[str, Any]:
    output_dir = ensure_dir(output_dir)
    common = {
        "schema_version": AB_PLAN_RESULT_SCHEMA_VERSION,
        "task_id": task["task_id"],
        "mode": mode,
        "model": model,
        "agent_scaffold": agent_scaffold,
        "evaluator": evaluator,
        "budget": budget,
        "public_claim": False,
        "real_patch_quality_claim": False,
        "quality_claim": False,
        "leakage_audit_pass": bool(sanitized_task.get("leakage_audit", {}).get("pass")),
        "sanitized_task_sha": _stable_json_sha(sanitized_task),
        "agent_visible_prompt": build_agent_visible_prompt(sanitized_task, mode),
    }
    common["agent_visible_leakage_findings"] = evaluator_only_field_findings(
        _loads_json_object(common["agent_visible_prompt"])
    )
    common["agent_visible_leakage_pass"] = not common["agent_visible_leakage_findings"]
    if mode in PATCH_MODES:
        result = _patch_mode_result(common, task, mode, enable_patch_modes, external_agent_command)
    elif mode == "mock_agent_scaffold_only":
        result = _mock_agent_result(common, sanitized_task)
    else:
        result = _plan_mode_result(common, task, sanitized_task, mode, codegraph_available)
    _write_mode_outputs(output_dir, result)
    return result


def validate_same_agent_invariant(run_plan: dict[str, Any], results: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    arm_a = run_plan.get("arms", {}).get("A", {})
    arm_b = run_plan.get("arms", {}).get("B", {})
    fixed = ("model", "agent_scaffold", "evaluator", "budget")
    mismatches = [field for field in fixed if arm_a.get(field) != arm_b.get(field)]
    a_tools = set(arm_a.get("tools", []))
    b_tools = set(arm_b.get("tools", []))
    normal_tools_preserved = set(NORMAL_TOOLS).issubset(a_tools) and set(NORMAL_TOOLS).issubset(b_tools)
    codegraph_only_extra = b_tools - a_tools == {CODEGRAPH_TOOL}
    visible_input_mismatch = []
    if results:
        by_pair: dict[str, dict[str, str]] = {}
        for result in results:
            mode = result.get("mode")
            if mode not in {"rg_only_agent_plan", "rg_plus_codegraph_agent_plan"}:
                continue
            by_pair.setdefault(str(result.get("task_id")), {})[str(mode)] = str(result.get("sanitized_task_sha"))
        visible_input_mismatch = [
            task_id
            for task_id, hashes in by_pair.items()
            if hashes.get("rg_only_agent_plan") and hashes.get("rg_plus_codegraph_agent_plan")
            and hashes.get("rg_only_agent_plan") != hashes.get("rg_plus_codegraph_agent_plan")
        ]
    return {
        "pass": not mismatches and normal_tools_preserved and codegraph_only_extra and not visible_input_mismatch,
        "metadata_mismatches": mismatches,
        "normal_tools_preserved": normal_tools_preserved,
        "codegraph_only_extra_capability": codegraph_only_extra,
        "visible_input_mismatch_task_ids": visible_input_mismatch,
    }


def build_agent_visible_prompt(sanitized_task: dict[str, Any], mode: str) -> str:
    tools = list(NORMAL_TOOLS)
    if _mode_uses_codegraph(mode):
        tools.append(CODEGRAPH_TOOL)
    visible = {
        "task_id": sanitized_task.get("task_id"),
        "task": sanitized_task.get("task") or sanitized_task.get("prompt") or sanitized_task.get("task_text"),
        "visible_query_terms": sanitized_task.get("visible_query_terms", []),
        "visible_file_hints": sanitized_task.get("visible_file_hints", []),
        "visible_symbol_hints": sanitized_task.get("visible_symbol_hints", []),
        "available_tools": tools,
    }
    return json.dumps(visible, sort_keys=True)


def evaluator_only_field_findings(value: Any, *, path: str = "$") -> list[str]:
    findings: list[str] = []
    if isinstance(value, dict):
        for key, child in value.items():
            child_path = f"{path}.{key}"
            if key in EVALUATOR_ONLY_FIELDS:
                findings.append(child_path)
            findings.extend(evaluator_only_field_findings(child, path=child_path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            findings.extend(evaluator_only_field_findings(child, path=f"{path}[{index}]"))
    return findings


def _loads_json_object(value: str) -> Any:
    try:
        return json.loads(value)
    except json.JSONDecodeError:
        return value


def _plan_mode_result(common: dict[str, Any], task: dict[str, Any], sanitized_task: dict[str, Any], mode: str, codegraph_available: bool) -> dict[str, Any]:
    if _mode_uses_codegraph(mode) and not codegraph_available:
        return {
            **common,
            "status": "blocked",
            "blocked_reason": "CodeGraph context is unavailable; B arm must not get a silent advantage or fallback quality claim.",
            "tools_available": _tools_for_mode(mode),
            "codegraph_availability": {"available": False, "status": "blocked"},
            "patch_mode_enabled": False,
        }
    context = build_context_bundle(sanitized_task, mode)
    plan = deterministic_plan_from_visible_task(sanitized_task, mode, context)
    scorer_input = {**plan, **context.get("scorer_overlay", {})}
    plan_score = plan_accuracy(task, scorer_input)
    proof_score = proof_discipline(task, scorer_input)
    hallucination_score = hallucination_trap(task, scorer_input)
    return {
        **common,
        "status": "diagnostic_only" if mode == "deterministic_plan_only" else "completed",
        "blocked_reason": "",
        "tools_available": _tools_for_mode(mode),
        "codegraph_availability": {"available": _mode_uses_codegraph(mode), "status": "completed" if _mode_uses_codegraph(mode) else "not_applicable"},
        "context": context,
        "agent_plan": plan,
        "evidence_references": context["evidence_references"],
        "tool_calls": context["tool_calls"],
        "context_bytes": context["context_bytes"],
        "scores": {
            "plan_accuracy": plan_score,
            "proof_discipline": proof_score,
            "hallucination_trap": hallucination_score,
        },
        "scorer_failure_reasons": plan_score["reasons"] + proof_score["reasons"] + hallucination_score["reasons"],
        "patch_mode_enabled": False,
        "claim_boundary": "plan-only diagnostic; no patch-quality claim",
    }


def _patch_mode_result(common: dict[str, Any], task: dict[str, Any], mode: str, enable_patch_modes: bool, external_agent_command: str) -> dict[str, Any]:
    placeholder = patch_outcome_placeholder(
        task,
        enabled=bool(enable_patch_modes and external_agent_command),
        external_agent_harness_configured=bool(external_agent_command),
    )
    if not enable_patch_modes:
        status = "not_configured"
        reason = "patch mode disabled by default; requires a later explicit patch-quality prompt"
    elif not external_agent_command:
        status = "blocked"
        reason = "external agent command is not configured"
    else:
        status = "blocked"
        reason = "external agent command readiness is scaffolded; this prompt does not execute patch-quality runs"
    return {
        **common,
        "status": status,
        "blocked_reason": reason,
        "tools_available": _tools_for_mode(mode),
        "patch_mode_enabled": False,
        "patch_outcome_placeholder": placeholder,
        "patch_diff": None,
        "tests_run": [],
        "resolved_status": "not_applicable",
        "wrong_file_edits": "not_applicable",
        "nonexistent_symbol_references": "not_applicable",
        "unsupported_claims": "not_applicable",
        "evidence_alignment": "not_applicable",
        "cost_time_tool_calls": "not_applicable",
        "context_attribution_validity": "not_applicable",
        "claim_boundary": "patch mode scaffold only; no external-agent patch-quality execution",
    }


def _mock_agent_result(common: dict[str, Any], sanitized_task: dict[str, Any]) -> dict[str, Any]:
    agent = MockAgent()
    context = {"tools_available": list(NORMAL_TOOLS), "scaffold_only": True}
    raw = agent.run(sanitized_task, context)
    return {
        **common,
        "status": "scaffold_only",
        "blocked_reason": "mock-agent mode is plumbing only and never product quality",
        "tools_available": list(NORMAL_TOOLS),
        "mock_agent_status": raw.status,
        "quality_claim": False,
        "agent_plan": None,
        "patch_mode_enabled": False,
        "claim_boundary": "mock-agent scaffold only; not product quality",
    }


def build_context_bundle(sanitized_task: dict[str, Any], mode: str) -> dict[str, Any]:
    visible_files = [str(value) for value in sanitized_task.get("visible_file_hints", [])]
    visible_symbols = [str(value) for value in sanitized_task.get("visible_symbol_hints", [])]
    visible_terms = [str(value) for value in sanitized_task.get("visible_query_terms", [])]
    tool_calls = [
        {"tool": "rg", "kind": "file_discovery", "argv": None, "source": "deterministic_visible_fixture"},
        {"tool": "search", "kind": "visible_query_terms", "argv": None, "terms": visible_terms[:6]},
    ]
    evidence = [{"source": "visible_file_hint", "file": file_path} for file_path in visible_files]
    if _mode_uses_codegraph(mode):
        tool_calls.append({"tool": "codegraph", "kind": "routing_packet", "argv": None, "source": "deterministic_visible_fixture"})
        evidence.append(
            {
                "source": "codegraph_routing_packet_scaffold",
                "proof_strength": "source_navigation_evidence",
                "graph_proof": False,
            }
        )
    context_bytes = len(json.dumps({"files": visible_files, "symbols": visible_symbols, "terms": visible_terms}).encode("utf-8"))
    proof_ladder = ["text_evidence", "source_navigation_evidence", "no_proof_path_found"]
    if _mode_uses_codegraph(mode):
        proof_ladder = ["source_navigation_evidence", "text_evidence", "no_proof_path_found"]
    return {
        "rg_search_outputs": {
            "files": visible_files,
            "symbols": visible_symbols,
            "visible_terms": visible_terms,
        },
        "codegraph_context": {
            "available": _mode_uses_codegraph(mode),
            "routing_packet": {
                "task_intent": "deterministic_visible_plan",
                "critical_files": visible_files,
                "critical_symbols": visible_symbols,
                "proof_ladder": proof_ladder,
                "proof_status": "no_proof_path_found",
            }
            if _mode_uses_codegraph(mode)
            else None,
        },
        "evidence_references": evidence,
        "tool_calls": tool_calls,
        "context_bytes": context_bytes,
        "scorer_overlay": {
            "proof_status": "no_proof_path_found",
            "proof_ladder": proof_ladder,
            "graph_proof": False,
        },
    }


def deterministic_plan_from_visible_task(sanitized_task: dict[str, Any], mode: str, context: dict[str, Any]) -> dict[str, Any]:
    visible_files = list(context["rg_search_outputs"]["files"])
    visible_symbols = list(context["rg_search_outputs"]["symbols"])
    tests = _visible_tests(sanitized_task)
    unknowns = _visible_unknowns(sanitized_task)
    validation_steps = _visible_validation_steps(sanitized_task, mode)
    plan_facts = _visible_plan_facts(sanitized_task, mode)
    return {
        "plan_schema_version": "deterministic_plan_from_visible_task_v1",
        "mode": mode,
        "implementation_surface": visible_files,
        "edit_targets": visible_files,
        "symbols": visible_symbols,
        "tests": tests,
        "unknowns": unknowns,
        "validation_steps": validation_steps,
        "plan_facts": plan_facts,
        "claims": [
            "This is a plan-only diagnostic generated from provider-visible task fields.",
            "No patch-quality or product-value claim is made.",
        ],
        "confidence": "bounded",
        "proof_status": "no_proof_path_found",
        "proof_ladder": context["scorer_overlay"]["proof_ladder"],
        "graph_proof": False,
    }


def _visible_tests(sanitized_task: dict[str, Any]) -> list[str]:
    text = str(sanitized_task.get("task") or sanitized_task.get("prompt") or "")
    if "test" in text.lower():
        return ["targeted tests named by the visible task prompt"]
    return ["targeted tests for visible implementation surface"]


def _visible_unknowns(sanitized_task: dict[str, Any]) -> list[str]:
    text = str(sanitized_task.get("task") or sanitized_task.get("prompt") or "").lower()
    unknowns = []
    if any(token in text for token in ("same", "ambig", "renamed", "deleted", "dynamic", "stale", "config", "auth")):
        unknowns.append("visible ambiguity or runtime boundary must be verified")
    if not unknowns:
        unknowns.append("implementation surface must be verified from live source before editing")
    return unknowns


def _visible_validation_steps(sanitized_task: dict[str, Any], mode: str) -> list[str]:
    steps = ["inspect visible file hints", "run targeted tests for visible implementation surface"]
    if _mode_uses_codegraph(mode):
        steps.insert(1, "inspect CodeGraph routing packet proof labels")
    return steps


def _visible_plan_facts(sanitized_task: dict[str, Any], mode: str) -> list[str]:
    files = ", ".join(str(value) for value in sanitized_task.get("visible_file_hints", [])[:3])
    prefix = "CodeGraph-added" if _mode_uses_codegraph(mode) else "rg-only"
    return [f"{prefix} plan uses visible files: {files}" if files else f"{prefix} plan has no visible file hints"]


def _arm_config(
    arm: str,
    *,
    uses_codegraph: bool,
    model: str,
    agent_scaffold: str,
    evaluator: str,
    budget: dict[str, Any],
) -> dict[str, Any]:
    tools = list(NORMAL_TOOLS)
    if uses_codegraph:
        tools.append(CODEGRAPH_TOOL)
    return {
        "arm": arm,
        "model": model,
        "agent_scaffold": agent_scaffold,
        "evaluator": evaluator,
        "budget": budget,
        "tools": tools,
        "uses_codegraph": uses_codegraph,
    }


def _mode_plan(mode: str, *, enable_patch_modes: bool, external_agent_command: str, codegraph_available: bool) -> dict[str, Any]:
    if mode not in AB_HARNESS_MODES:
        return {"mode": mode, "status": "failed", "reason": "unknown mode"}
    if mode in PATCH_MODES:
        status = "not_configured" if not enable_patch_modes else ("blocked" if not external_agent_command else "blocked")
        reason = "patch modes disabled by default" if not enable_patch_modes else "patch mode scaffold does not execute in this prompt"
    elif mode == "mock_agent_scaffold_only":
        status = "scaffold_only"
        reason = "mock-agent mode is scaffold-only"
    elif _mode_uses_codegraph(mode) and not codegraph_available:
        status = "blocked"
        reason = "CodeGraph unavailable"
    else:
        status = "diagnostic_only" if mode == "deterministic_plan_only" else "completed"
        reason = "plan-only diagnostic"
    return {
        "mode": mode,
        "status": status,
        "reason": reason,
        "tools_available": _tools_for_mode(mode),
        "patch_quality_claim": False,
    }


def _mode_uses_codegraph(mode: str) -> bool:
    return mode == "rg_plus_codegraph_agent_plan" or mode == "rg_plus_codegraph_agent_patch"


def _tools_for_mode(mode: str) -> list[str]:
    tools = list(NORMAL_TOOLS)
    if _mode_uses_codegraph(mode):
        tools.append(CODEGRAPH_TOOL)
    return tools


def _write_mode_outputs(output_dir: Path, result: dict[str, Any]) -> None:
    _write_json(output_dir / "agent_plan.json", result)
    if result.get("agent_plan") is not None:
        lines = [
            f"# Agent Plan: {result['task_id']} / {result['mode']}",
            "",
            "Local plan-only diagnostic. No patch-quality claim.",
            "",
            "## Implementation Surface",
            "",
        ]
        for file_path in result["agent_plan"].get("implementation_surface", []):
            lines.append(f"- {file_path}")
        lines.extend(["", "## Validation Steps", ""])
        for step in result["agent_plan"].get("validation_steps", []):
            lines.append(f"- {step}")
        lines.append("")
        (output_dir / "agent_plan.md").write_text("\n".join(lines), encoding="utf-8")
    else:
        (output_dir / "agent_plan.md").write_text("No agent plan generated for this scaffold mode.\n", encoding="utf-8")


def _mode_error(task: dict[str, Any], mode: str, reason: str) -> dict[str, Any]:
    return {
        "schema_version": AB_PLAN_RESULT_SCHEMA_VERSION,
        "task_id": task.get("task_id"),
        "mode": mode,
        "status": "failed",
        "blocked_reason": reason,
        "public_claim": False,
        "real_patch_quality_claim": False,
    }


def _mode_counts(results: list[dict[str, Any]]) -> dict[str, dict[str, int]]:
    counts: dict[str, dict[str, int]] = {}
    for result in results:
        mode = str(result.get("mode"))
        status = str(result.get("status"))
        counts.setdefault(mode, {})
        counts[mode][status] = counts[mode].get(status, 0) + 1
    return counts


def _stable_json_sha(value: Any) -> str:
    import hashlib

    payload = json.dumps(value, sort_keys=True, separators=(",", ":"), default=str).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def _git_commit_or_unknown() -> str:
    head = Path(".git/HEAD")
    if not head.exists():
        return "unknown"
    try:
        text = head.read_text(encoding="utf-8").strip()
        if text.startswith("ref:"):
            ref = Path(".git") / text.removeprefix("ref:").strip()
            return ref.read_text(encoding="utf-8").strip() if ref.exists() else "unknown"
        return text
    except OSError:
        return "unknown"


def _write_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
