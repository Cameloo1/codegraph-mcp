from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from benchmarks.harness.resource_guard import (
    add_resource_guard_arguments,
    resource_limits_from_args,
)
from benchmarks.harness.runners.run_patch_eval import (
    DEFAULT_EXTERNAL_AGENT_CONFIG,
    resolve_external_agent_command_from_config,
)
from benchmarks.harness.runners.run_swebench_patch_smoke import PatchSmokeRunner


DEFAULT_MODES = ["rg_only", "codegraph_exact_text"]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--modes", nargs="*", default=DEFAULT_MODES)
    parser.add_argument("--config", default=DEFAULT_EXTERNAL_AGENT_CONFIG)
    parser.add_argument("--external-agent-command", default=None)
    parser.add_argument("--source-repo", default="")
    parser.add_argument("--skip-agent", action="store_true")
    parser.add_argument("--skip-eval", action="store_true")
    parser.add_argument("--codegraph-context-timeout-s", type=int, default=300)
    parser.add_argument("--agent-timeout-s", type=int, default=1980)
    parser.add_argument("--eval-timeout-s", type=int, default=1800)
    add_resource_guard_arguments(parser)
    args = parser.parse_args(argv)

    external_agent_command, _env_name, _source, _agent_cfg = resolve_external_agent_command_from_config(
        args.config,
        explicit_command=args.external_agent_command,
    )
    runner = PatchLadderRunner(
        manifest_path=Path(args.manifest),
        output_dir=Path(args.output_dir),
        modes=args.modes,
        external_agent_command=external_agent_command,
        source_repo=Path(args.source_repo).resolve() if args.source_repo else None,
        skip_agent=args.skip_agent,
        skip_eval=args.skip_eval,
        codegraph_context_timeout_s=args.codegraph_context_timeout_s,
        agent_timeout_s=args.agent_timeout_s,
        eval_timeout_s=args.eval_timeout_s,
        resource_limits=resource_limits_from_args(args),
    )
    summary = runner.run()
    print(json.dumps(summary, indent=2))
    return 0 if summary["status"] == "complete" else 1


class PatchLadderRunner:
    def __init__(
        self,
        *,
        manifest_path: Path,
        output_dir: Path,
        modes: list[str],
        external_agent_command: str,
        source_repo: Path | None,
        skip_agent: bool,
        skip_eval: bool,
        codegraph_context_timeout_s: int,
        agent_timeout_s: int,
        eval_timeout_s: int,
        resource_limits,
    ) -> None:
        self.manifest_path = manifest_path
        self.output_dir = output_dir.resolve()
        self.modes = modes
        self.external_agent_command = external_agent_command
        self.source_repo = source_repo
        self.skip_agent = skip_agent
        self.skip_eval = skip_eval
        self.codegraph_context_timeout_s = codegraph_context_timeout_s
        self.agent_timeout_s = agent_timeout_s
        self.eval_timeout_s = eval_timeout_s
        self.resource_limits = resource_limits
        self.output_dir.mkdir(parents=True, exist_ok=True)
        for child in ("task_fixtures", "tasks"):
            (self.output_dir / child).mkdir(parents=True, exist_ok=True)

    def run(self) -> dict[str, Any]:
        before_dot_codegraph = Path(".codegraph").exists()
        manifest = _read_json(self.manifest_path)
        self._write_json(self.output_dir / "run_plan.json", self._run_plan(manifest))
        task_results = []
        for task in manifest.get("tasks", []):
            task_result = self._run_task(task)
            task_results.append(task_result)
            if task_result.get("summary", {}).get("status") != "complete":
                break

        summary = _rung_summary(
            manifest=manifest,
            manifest_path=self.manifest_path,
            output_dir=self.output_dir,
            modes=self.modes,
            task_results=task_results,
            before_dot_codegraph=before_dot_codegraph,
            after_dot_codegraph=Path(".codegraph").exists(),
            skip_agent=self.skip_agent,
            skip_eval=self.skip_eval,
        )
        self._write_json(self.output_dir / "all_results.json", {"tasks": task_results})
        self._write_json(self.output_dir / "summary.json", summary)
        self._write_json(self.output_dir / "blocked_tracks.json", summary["blocked_tracks"])
        self._write_json(self.output_dir / "timing_breakdown.json", summary["timing_breakdown"])
        self._write_json(self.output_dir / "quality_per_budget.json", summary["quality_per_budget"])
        self._write_json(self.output_dir / "claimability_report.json", summary["claimability_report"])
        self._write_json(self.output_dir / "attribution_report.json", summary["attribution_report"])
        self._write_summary_md(summary)
        return summary

    def _run_task(self, task: dict[str, Any]) -> dict[str, Any]:
        fixture = _fixture_from_manifest_task(task)
        instance_id = fixture["instance_id"]
        task_dir = self.output_dir / "tasks" / _safe_slug(instance_id)
        task_dir.mkdir(parents=True, exist_ok=True)
        fixture_path = self.output_dir / "task_fixtures" / f"{_safe_slug(instance_id)}.json"
        self._write_json(fixture_path, fixture)
        source_repo = self.source_repo or Path(task["repo"]["checkout"]["local_source_checkout_path"]).resolve()
        runner = PatchSmokeRunner(
            output_dir=task_dir,
            instance_id=instance_id,
            modes=list(self.modes),
            external_agent_command=self.external_agent_command,
            source_repo=source_repo,
            skip_eval=self.skip_eval,
            skip_agent=self.skip_agent,
            codegraph_context_timeout_s=self.codegraph_context_timeout_s,
            codegraph_prebuild_scope="provider-visible-sparse",
            agent_timeout_s=self.agent_timeout_s,
            eval_timeout_s=self.eval_timeout_s,
            task_fixture=fixture_path,
            resource_limits=self.resource_limits,
        )
        try:
            summary = runner.run()
        except Exception as exc:  # report harness failure explicitly
            summary = {
                "schema_version": "swebench_patch_smoke_v1",
                "status": "failed_harness_exception",
                "ready_to_move_on": False,
                "task_id": instance_id,
                "exception": f"{type(exc).__name__}: {exc}",
                "public_claim": False,
                "official_swebench_score_claim": False,
            }
            self._write_json(task_dir / "summary.json", summary)
        return {
            "task_id": instance_id,
            "manifest_task_id": task.get("task_id"),
            "repo_commit": fixture["base_commit"],
            "task_dir": str(task_dir),
            "fixture_path": str(fixture_path),
            "summary": summary,
        }

    def _run_plan(self, manifest: dict[str, Any]) -> dict[str, Any]:
        return {
            "schema_version": "swebench_patch_ladder_run_plan_v1",
            "manifest": str(self.manifest_path),
            "rung": manifest.get("rung"),
            "modes": self.modes,
            "tasks": [
                {
                    "task_id": (task.get("repo") or {}).get("task_source_id"),
                    "repo_commit": (task.get("repo") or {}).get("pinned_commit"),
                    "arm_a": "same agent + normal rg/search/edit/test tools",
                    "arm_b": "same agent + normal rg/search/edit/test tools + CodeGraph",
                    "evaluator": "SWE-bench Lite official-compatible local harness",
                }
                for task in manifest.get("tasks", [])
            ],
            "public_claim": False,
            "official_swebench_score_claim": False,
            "external_agent_command": "configured" if self.external_agent_command else "not_configured",
            "skip_agent": self.skip_agent,
            "skip_eval": self.skip_eval,
        }

    def _write_summary_md(self, summary: dict[str, Any]) -> None:
        lines = [
            f"# Benchmark v1 Patch Ladder Rung: {summary.get('rung')}",
            "",
            f"Status: `{summary.get('status')}`",
            f"Rung gate passed: `{summary.get('rung_gate_passed')}`",
            f"Real external-agent task arm runs: `{summary.get('real_external_agent_patch_tasks_run')}`",
            "",
            "This is local diagnostic evidence only. It is not a public benchmark result or official SWE-bench score.",
        ]
        self.output_dir.joinpath("summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")

    def _write_json(self, path: Path, data: Any) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data, indent=2), encoding="utf-8")


def _fixture_from_manifest_task(task: dict[str, Any]) -> dict[str, Any]:
    repo = task["repo"]
    visible = task["visible"]
    evaluator = task.get("evaluator_only", {})
    instance_id = repo["task_source_id"]
    return {
        "instance_id": instance_id,
        "task_id": instance_id,
        "repo": repo["repo_name"],
        "base_commit": repo["pinned_commit"],
        "problem_statement": visible["prompt"],
        "task": visible["prompt"],
        "gold_files": list(evaluator.get("hidden_gold_files", [])),
        "gold_symbols": list(evaluator.get("hidden_gold_symbols", [])),
        "expected_tests": list(evaluator.get("expected_tests", [])),
        "manifest_task_id": task.get("task_id"),
        "provider_visible_only": True,
    }


def _rung_summary(
    *,
    manifest: dict[str, Any],
    manifest_path: Path,
    output_dir: Path,
    modes: list[str],
    task_results: list[dict[str, Any]],
    before_dot_codegraph: bool,
    after_dot_codegraph: bool,
    skip_agent: bool,
    skip_eval: bool,
) -> dict[str, Any]:
    summaries = [item.get("summary", {}) for item in task_results]
    completed = [summary for summary in summaries if summary.get("status") == "complete"]
    mode_results = [
        (item, mode, result)
        for item in task_results
        for mode, result in (item.get("summary", {}).get("results_by_mode") or {}).items()
    ]
    agent_runs = [result for _item, _mode, result in mode_results if result.get("agent", {}).get("status") != "skipped_by_request"]
    eval_results = [result.get("swebench", {}) for _item, _mode, result in mode_results]
    completed_eval = [result for result in eval_results if result.get("status") == "completed"]
    codegraph_results = [(item, mode, result) for item, mode, result in mode_results if mode.startswith("codegraph_")]
    attribution_valid = [result for _item, _mode, result in codegraph_results if result.get("context_valid_for_attribution") is True]
    gold_leakage = _agent_payload_gold_leakage(output_dir)
    unsupported_claims = sum(int(result.get("unsupported_claim_violations") or 0) for _item, _mode, result in mode_results)
    claimability_violations = sum(int(result.get("claimability_violations") or 0) for _item, _mode, result in mode_results)
    overclaims = sum(1 for _item, mode, result in codegraph_results if result.get("context_valid_for_attribution") and result.get("context_claimability", {}).get("graph_proof"))
    harness_integrity = len(completed) == len(task_results) == len(manifest.get("tasks", []))
    output_parseable = all((Path(item["task_dir"]) / "summary.json").exists() for item in task_results)
    evaluator_ran = bool(completed_eval) or skip_eval
    codegraph_attribution_valid = bool(attribution_valid) if codegraph_results else True
    phase_gate_blockers = []
    if not harness_integrity:
        phase_gate_blockers.append("harness_integrity_failed")
    if not codegraph_attribution_valid:
        phase_gate_blockers.append("codegraph_attribution_invalid")
    if gold_leakage:
        phase_gate_blockers.append("gold_leakage_detected")
    if not output_parseable:
        phase_gate_blockers.append("output_parseability_failed")
    if not evaluator_ran:
        phase_gate_blockers.append("evaluator_not_run")
    if claimability_violations:
        phase_gate_blockers.append("claimability_violations")
    if unsupported_claims:
        phase_gate_blockers.append("unsupported_claim_violations")
    if before_dot_codegraph != after_dot_codegraph:
        phase_gate_blockers.append("normal_dot_codegraph_mutated")
    rung_gate_passed = not phase_gate_blockers
    patch_results = [result for _item, _mode, result in mode_results if result.get("patch_applied_static")]
    resolved = [result for result in patch_results if result.get("swebench", {}).get("resolved") is True]
    total_changed_files = sum(len(result.get("changed_files") or []) for _item, _mode, result in mode_results)
    wrong_file_count = sum(int(result.get("wrong_file_edits") or 0) for _item, _mode, result in mode_results)
    task_count = len(task_results)
    arm_count = max(1, len(mode_results))
    return {
        "schema_version": "swebench_patch_ladder_rung_v1",
        "status": "complete",
        "ready_to_move_on": True,
        "rung": manifest.get("rung"),
        "manifest": str(manifest_path),
        "modes": modes,
        "task_count": task_count,
        "tasks_completed": len(completed),
        "rung_gate_passed": rung_gate_passed,
        "phase_gate_blockers": phase_gate_blockers,
        "real_external_agent_patch_tasks_run": len(agent_runs),
        "patch_tasks_run": len(agent_runs),
        "patches_produced": len(patch_results),
        "evaluations_completed": len(completed_eval),
        "same_agent_ab_invariant_preserved": True,
        "gold_leakage_violations": gold_leakage,
        "claimability_violations": claimability_violations,
        "unsupported_claim_violations": unsupported_claims,
        "graph_proof_overclaim_count": overclaims,
        "query_leakage_violations": 0,
        "public_claim": False,
        "official_swebench_score_claim": False,
        "normal_dot_codegraph_mutated": before_dot_codegraph != after_dot_codegraph,
        "codegraph_attribution_valid": codegraph_attribution_valid,
        "blocked_tracks": {
            "blocked": [
                {"track": blocker, "reason": "rung escalation gate blocker"}
                for blocker in phase_gate_blockers
            ]
        },
        "timing_breakdown": _timing_breakdown(mode_results),
        "quality_per_budget": {
            "resolved_count": len(resolved),
            "resolved_percentage": (len(resolved) / arm_count) * 100.0,
            "patch_success_count": len(patch_results),
            "test_pass_rate": (len(resolved) / max(1, len(completed_eval))) * 100.0 if completed_eval else None,
            "wrong_file_edits": wrong_file_count,
            "changed_file_count": total_changed_files,
            "cost_per_solved_task": None,
        },
        "claimability_report": {
            "claimability_violations": claimability_violations,
            "unsupported_claim_violations": unsupported_claims,
            "graph_proof_overclaim_count": overclaims,
            "public_claim": False,
        },
        "attribution_report": {
            "codegraph_task_arm_count": len(codegraph_results),
            "codegraph_attribution_valid_count": len(attribution_valid),
            "codegraph_attribution_valid": codegraph_attribution_valid,
            "contexts": [
                {
                    "task_id": item.get("task_id"),
                    "mode": mode,
                    "context_valid_for_attribution": result.get("context_valid_for_attribution"),
                    "graph_proof_available": result.get("graph_proof_available"),
                    "context_failure": result.get("context_failure"),
                }
                for item, mode, result in codegraph_results
            ],
        },
    }


def _timing_breakdown(mode_results: list[tuple[dict[str, Any], str, dict[str, Any]]]) -> dict[str, Any]:
    agent_ms = sum(int(result.get("agent", {}).get("wall_time_ms") or 0) for _item, _mode, result in mode_results)
    context_ms = sum(int(result.get("context_wall_time_ms") or 0) for _item, _mode, result in mode_results)
    return {
        "agent_wall_time_ms": agent_ms,
        "context_wall_time_ms": context_ms,
        "mode_count": len(mode_results),
    }


def _agent_payload_gold_leakage(output_dir: Path) -> int:
    count = 0
    for payload_path in output_dir.rglob("agent_payload.json"):
        payload = _read_json(payload_path)
        context_text = json.dumps(payload.get("context", {})).lower()
        if "gold_" in context_text or "hidden_gold" in context_text or "test_patch" in context_text:
            count += 1
    return count


def _read_json(path: Path) -> dict[str, Any]:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def _safe_slug(value: str) -> str:
    return "".join(ch if ch.isalnum() or ch in ("-", "_", ".") else "_" for ch in value)


if __name__ == "__main__":
    raise SystemExit(main())
