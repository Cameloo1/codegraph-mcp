from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import time
from pathlib import Path
from typing import Any

from benchmarks.harness.resource_guard import (
    ResourceLimits,
    add_resource_guard_arguments,
    resource_limits_from_args,
    resource_limits_from_env,
    run_command_with_resource_guard,
    summarize_resource_limit_failures,
)
from benchmarks.harness.paths import fixture_path, resolve_benchmark_path
from benchmarks.harness.scoring.patch_outcome import changed_files_from_patch, wrong_file_edits


ROOT = Path(__file__).resolve().parents[3]
TASK_ID = "sympy__sympy-20590"
BASE_COMMIT = "cffd4e0f86fefd4802349a9f9b19ed70934ea354"
MODES = ["baseline", "rg_only", "codegraph_exact_text", "codegraph_full"]
MAX_CONTEXT_BYTES = 120_000
MAX_TIME_S = 1800
CODEGRAPH_CONTEXT_TIMEOUT_S = 240
DEFAULT_AGENT_TIMEOUT_S = MAX_TIME_S + 180
MAX_TOOL_CALLS = 80
DEFAULT_TASK_FIXTURE = ROOT / fixture_path("swebench_lite", "swebench_lite_sympy_20590.json")
LEGACY_TASK_CACHE = (
    ROOT
    / "benchmarks"
    / "results"
    / "summaries"
    / "swebench_focused_20260521_163620"
    / "per_task"
    / TASK_ID
    / "task.json"
)


class BenchmarkBlocked(RuntimeError):
    pass


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--instance-id", default=TASK_ID)
    parser.add_argument("--modes", nargs="*", default=MODES)
    parser.add_argument("--external-agent-command", default=os.environ.get("CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND", ""))
    parser.add_argument("--source-repo", default="")
    parser.add_argument("--skip-eval", action="store_true")
    parser.add_argument("--skip-agent", action="store_true")
    parser.add_argument("--codegraph-context-timeout-s", type=int, default=CODEGRAPH_CONTEXT_TIMEOUT_S)
    parser.add_argument("--agent-timeout-s", type=int, default=DEFAULT_AGENT_TIMEOUT_S)
    parser.add_argument("--task-fixture", default=str(DEFAULT_TASK_FIXTURE))
    add_resource_guard_arguments(parser)
    args = parser.parse_args(argv)

    output_dir = Path(args.output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    runner = PatchSmokeRunner(
        output_dir=output_dir,
        instance_id=args.instance_id,
        modes=args.modes,
        external_agent_command=args.external_agent_command,
        source_repo=Path(args.source_repo).resolve() if args.source_repo else None,
        skip_eval=args.skip_eval,
        skip_agent=args.skip_agent,
        codegraph_context_timeout_s=args.codegraph_context_timeout_s,
        agent_timeout_s=args.agent_timeout_s,
        task_fixture=resolve_benchmark_path(args.task_fixture).resolve() if args.task_fixture else None,
        resource_limits=resource_limits_from_args(args),
    )
    summary = runner.run()
    print(json.dumps(summary, indent=2))
    return 0 if summary["status"] == "complete" else 1


class PatchSmokeRunner:
    def __init__(
        self,
        *,
        output_dir: Path,
        instance_id: str,
        modes: list[str],
        external_agent_command: str,
        source_repo: Path | None,
        skip_eval: bool,
        skip_agent: bool,
        codegraph_context_timeout_s: int,
        agent_timeout_s: int,
        task_fixture: Path | None,
        resource_limits: ResourceLimits | None = None,
    ) -> None:
        self.output_dir = output_dir
        self.instance_id = instance_id
        self.modes = modes
        self.external_agent_command = external_agent_command
        self.source_repo = source_repo or self._default_source_repo()
        self.skip_eval = skip_eval
        self.skip_agent = skip_agent
        self.codegraph_context_timeout_s = codegraph_context_timeout_s
        self.agent_timeout_s = agent_timeout_s
        self.task_fixture = task_fixture
        self.resource_limits = resource_limits or resource_limits_from_env()
        self.logs_dir = self.output_dir / "logs"
        self.repos_dir = self.output_dir / "repos"
        self.context_dir = self.output_dir / "context_packets"
        self.patches_dir = self.output_dir / "patches"
        self.predictions_dir = self.output_dir / "predictions"
        self.eval_dir = self.output_dir / "swebench_eval"
        for path in (self.logs_dir, self.repos_dir, self.context_dir, self.patches_dir, self.predictions_dir, self.eval_dir):
            path.mkdir(parents=True, exist_ok=True)
        self.commands_jsonl = self.output_dir / "commands.jsonl"
        self.commands_jsonl.write_text("", encoding="utf-8")

    def run(self) -> dict[str, Any]:
        before_dot_codegraph = (ROOT / ".codegraph").exists()
        try:
            task = self._load_task()
        except BenchmarkBlocked as exc:
            summary = self._blocked_summary(before_dot_codegraph, str(exc))
            self._write_json(self.output_dir / "summary.json", summary)
            self._write_summary_md(summary)
            return summary
        self._write_json(self.output_dir / "task.json", task)
        self._write_json(self.output_dir / "run_plan.json", self._run_plan(task))

        codegraph_prebuild = self._prebuild_codegraph_context(task) if any(mode.startswith("codegraph_") for mode in self.modes) else None
        mode_results: dict[str, dict[str, Any]] = {}
        prediction_paths: dict[str, Path] = {}
        for mode in self.modes:
            mode_result = self._run_mode(task, mode, codegraph_prebuild=codegraph_prebuild)
            mode_results[mode] = mode_result
            pred_path = mode_result.get("prediction_path")
            if pred_path:
                prediction_paths[mode] = Path(pred_path)

        eval_results = {} if self.skip_eval else self._run_swebench_eval(prediction_paths)
        for mode, result in mode_results.items():
            result["swebench"] = eval_results.get(mode, {"status": "not_run"})
            self._annotate_clean_patch_quality(result, task)

        after_dot_codegraph = (ROOT / ".codegraph").exists()
        summary = {
            "schema_version": "swebench_patch_smoke_v1",
            "status": "complete",
            "ready_to_move_on": True,
            "run_id": self.output_dir.name,
            "task_id": self.instance_id,
            "modes": self.modes,
            "public_claim": False,
            "official_swebench_score_claim": False,
            "claim_boundary": "local one-task SWE-bench Lite patch-quality smoke only",
            "normal_dot_codegraph_mutated": before_dot_codegraph != after_dot_codegraph,
            "external_agent_command_configured": bool(self.external_agent_command),
            "source_repo": str(self.source_repo),
            "results_by_mode": mode_results,
            "commands_jsonl": str(self.commands_jsonl),
            "codegraph_prebuild_manifest": str(self.output_dir / "codegraph_prebuild_manifest.json")
            if codegraph_prebuild
            else None,
            "codegraph_prebuild_status": codegraph_prebuild.get("status") if codegraph_prebuild else None,
            "resource_guard": self.resource_limits.to_dict(),
            "resource_limit_failures": summarize_resource_limit_failures(_load_jsonl(self.commands_jsonl)),
        }
        self._write_json(self.output_dir / "summary.json", summary)
        self._write_summary_md(summary)
        return summary

    def _blocked_summary(self, before_dot_codegraph: bool, reason: str) -> dict[str, Any]:
        return {
            "schema_version": "swebench_patch_smoke_v1",
            "status": "blocked",
            "ready_to_move_on": False,
            "run_id": self.output_dir.name,
            "task_id": self.instance_id,
            "modes": self.modes,
            "public_claim": False,
            "official_swebench_score_claim": False,
            "claim_boundary": "local one-task SWE-bench Lite patch-quality smoke only",
            "normal_dot_codegraph_mutated": before_dot_codegraph != (ROOT / ".codegraph").exists(),
            "external_agent_command_configured": bool(self.external_agent_command),
            "source_repo": str(self.source_repo),
            "blocked_reason": reason,
            "results_by_mode": {},
            "commands_jsonl": str(self.commands_jsonl),
            "resource_guard": self.resource_limits.to_dict(),
            "resource_limit_failures": summarize_resource_limit_failures(_load_jsonl(self.commands_jsonl)),
        }

    def _run_mode(self, task: dict[str, Any], mode: str, *, codegraph_prebuild: dict[str, Any] | None = None) -> dict[str, Any]:
        if mode.startswith("codegraph_") and codegraph_prebuild:
            context_repo = Path(codegraph_prebuild["repo_path"])
            context = self._build_context(task, context_repo, mode, codegraph_prebuild=codegraph_prebuild)
            repo = context_repo
        else:
            repo = self._prepare_repo(mode)
            context = self._build_context(task, repo, mode, codegraph_prebuild=codegraph_prebuild)
        self._write_json(self.context_dir / f"{self.instance_id}_{mode}.json", context)
        if mode.startswith("codegraph_") and not context.get("context_valid_for_attribution", True):
            result = self._blocked_invalid_context_result(task, mode, repo, context)
            self._write_json(self.output_dir / "per_mode" / mode / "result_pre_eval.json", result)
            return result
        if mode.startswith("codegraph_"):
            repo = self._prepare_repo(mode)
        agent = self._run_external_agent(task, context, repo, mode)
        patch_text = agent.get("patch_text") or ""
        prediction_path = None
        if patch_text:
            patch_path = self.patches_dir / f"{self.instance_id}_{mode}.patch"
            patch_path.write_text(patch_text, encoding="utf-8")
            prediction_path = self.predictions_dir / f"{self.instance_id}_{mode}.jsonl"
            prediction = {
                "instance_id": self.instance_id,
                "model_name_or_path": f"codex_cli_{mode}",
                "model_patch": patch_text,
            }
            prediction_path.write_text(json.dumps(prediction) + "\n", encoding="utf-8")
        changed_files = changed_files_from_patch(patch_text)
        patch_quality = _patch_file_quality(patch_text, task["gold_files"])
        result = {
            "task_id": self.instance_id,
            "mode": mode,
            "repo_path": str(repo),
            "context_valid_for_attribution": context.get("context_valid_for_attribution", True),
            "context_failure": context.get("context_failure", ""),
            "context_bytes": context["raw_context_bytes"],
            "estimated_context_tokens": context["estimated_context_tokens"],
            "tool_calls": context["tool_calls"],
            "context_wall_time_ms": context["wall_time_ms"],
            "patch_applied_static": bool(patch_text),
            "patch_path": str(self.patches_dir / f"{self.instance_id}_{mode}.patch") if patch_text else None,
            "prediction_path": str(prediction_path) if prediction_path else None,
            "changed_files": changed_files,
            "wrong_file_edits": patch_quality["wrong_file_edits"] if patch_text else None,
            "test_file_edits": patch_quality["test_file_edits"],
            "source_file_edits": patch_quality["source_file_edits"],
            "extra_file_edits": patch_quality["extra_file_edits"],
            "clean_source_patch": None,
            "clean_source_patch_reason": "eval_not_run",
            "nonexistent_symbol_references": None,
            "nonexistent_symbol_reference_status": "not_configured_known_symbol_index",
            "agent": {key: value for key, value in agent.items() if key != "patch_text"},
            "claimability_violations": 0,
            "unsupported_claim_violations": 0,
        }
        self._write_json(self.output_dir / "per_mode" / mode / "result_pre_eval.json", result)
        return result

    def _blocked_invalid_context_result(self, task: dict[str, Any], mode: str, repo: Path, context: dict[str, Any]) -> dict[str, Any]:
        return {
            "task_id": self.instance_id,
            "mode": mode,
            "repo_path": str(repo),
            "context_valid_for_attribution": False,
            "context_failure": context.get("context_failure", "invalid_context"),
            "context_bytes": context["raw_context_bytes"],
            "estimated_context_tokens": context["estimated_context_tokens"],
            "tool_calls": context["tool_calls"],
            "context_wall_time_ms": context["wall_time_ms"],
            "patch_applied_static": False,
            "patch_path": None,
            "prediction_path": None,
            "changed_files": [],
            "wrong_file_edits": None,
            "test_file_edits": [],
            "source_file_edits": [],
            "extra_file_edits": [],
            "clean_source_patch": False,
            "clean_source_patch_reason": "invalid_context",
            "nonexistent_symbol_references": None,
            "nonexistent_symbol_reference_status": "not_run_invalid_context",
            "agent": {
                "status": "skipped_invalid_context",
                "exit_code": None,
                "wall_time_ms": 0,
                "stdout_bytes": 0,
                "stderr_bytes": 0,
                "stderr_tail": "",
                "patch_capture": "none",
            },
            "claimability_violations": 0,
            "unsupported_claim_violations": 0,
        }

    def _annotate_clean_patch_quality(self, result: dict[str, Any], task: dict[str, Any]) -> None:
        if not result.get("patch_applied_static"):
            result["clean_source_patch"] = False
            result["clean_source_patch_reason"] = result.get("context_failure") or "no_patch"
            return
        swebench = result.get("swebench", {})
        if swebench.get("status") == "not_run":
            result["clean_source_patch"] = None
            result["clean_source_patch_reason"] = "eval_not_run"
            return
        if swebench.get("resolved") is not True:
            result["clean_source_patch"] = False
            result["clean_source_patch_reason"] = "not_resolved"
            return
        if result.get("extra_file_edits"):
            result["clean_source_patch"] = False
            result["clean_source_patch_reason"] = "extra_file_edits"
            return
        gold_tests = [path for path in task.get("gold_files", []) if _is_test_path(path)]
        unexpected_tests = [path for path in result.get("test_file_edits", []) if path not in gold_tests]
        if unexpected_tests:
            result["clean_source_patch"] = False
            result["clean_source_patch_reason"] = "unexpected_test_file_edits"
            return
        result["clean_source_patch"] = True
        result["clean_source_patch_reason"] = "resolved_no_extra_or_unexpected_test_edits"

    def _prepare_repo(self, mode: str) -> Path:
        dest = self.repos_dir / _mode_repo_slug(mode)
        if dest.exists():
            shutil.rmtree(dest)
        if self.source_repo.exists():
            self._must(
                self._run_command(
                    [
                        "git",
                        "-c",
                        "core.longpaths=true",
                        "-c",
                        f"safe.directory={self.source_repo}",
                        "-c",
                        f"safe.directory={self.source_repo / '.git'}",
                        "clone",
                        "--no-hardlinks",
                        str(self.source_repo),
                        str(dest),
                    ],
                    f"clone_{mode}",
                    timeout_s=600,
                )
            )
        else:
            self._must(
                self._run_command(
                    ["git", "-c", "core.longpaths=true", "clone", "--filter=blob:none", "https://github.com/sympy/sympy.git", str(dest)],
                    f"clone_{mode}",
                    timeout_s=1200,
                )
            )
        git_base = ["git", "-c", "core.longpaths=true"]
        self._must(self._run_command([*git_base, "checkout", BASE_COMMIT], f"checkout_{mode}", cwd=dest, timeout_s=120))
        self._must(self._run_command([*git_base, "reset", "--hard", BASE_COMMIT], f"reset_{mode}", cwd=dest, timeout_s=120))
        self._must(self._run_command([*git_base, "clean", "-fdx"], f"clean_{mode}", cwd=dest, timeout_s=120))
        return dest

    def _build_context(
        self,
        task: dict[str, Any],
        repo: Path,
        mode: str,
        *,
        codegraph_prebuild: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        if mode == "baseline":
            text = "No extra context was supplied for this baseline mode."
            return self._context_packet(mode, text=text, files=[], tool_calls=0, wall_time_ms=0)
        if mode == "rg_only":
            return self._rg_context(repo, mode)
        if mode == "codegraph_exact_text":
            return self._codegraph_context(task, repo, mode, full=False, prebuild=codegraph_prebuild)
        if mode == "codegraph_full":
            return self._codegraph_context(task, repo, mode, full=True, prebuild=codegraph_prebuild)
        raise ValueError(f"unsupported mode: {mode}")

    def _rg_context(self, repo: Path, mode: str) -> dict[str, Any]:
        rg = ROOT / ".codex-tools" / "rg.exe"
        rg_base = [str(rg), "--no-ignore-parent"]
        commands = [
            [*rg_base, "--files"],
            [*rg_base, "-l", "--glob", "*.py", "__dict__|__slots__", "sympy"],
            [*rg_base, "-n", "--glob", "*.py", "__dict__|__slots__", "sympy"],
            [*rg_base, "-n", "--glob", "*.py", r"class (Symbol|Printable|Basic)\b", "sympy"],
        ]
        records = [self._run_command(command, f"{mode}_context_{idx:02d}", cwd=repo, timeout_s=120) for idx, command in enumerate(commands, 1)]
        hits: dict[str, int] = {}
        for record in records:
            for path in _files_from_rg(record["stdout"]):
                hits[path] = hits.get(path, 0) + 1
        selected = sorted(hits, key=lambda path: _score_rg_path(path, hits))[:8]
        if selected:
            records.append(
                self._run_command(
                    [*rg_base, "-n", "-C", "6", r"__dict__|__slots__|class Printable|class Symbol|class Basic", *selected],
                    f"{mode}_context_snippets",
                    cwd=repo,
                    timeout_s=120,
                )
            )
        text = _trim_context(records, MAX_CONTEXT_BYTES)
        return self._context_packet(
            mode,
            text=text,
            files=selected,
            tool_calls=len(records),
            wall_time_ms=sum(int(record["wall_time_ms"]) for record in records),
            rg_calls=len(records),
            claimability={"graph_proof": False, "proof_status": "source_text_evidence", "text_evidence_only": True},
        )

    def _prebuild_codegraph_context(self, task: dict[str, Any]) -> dict[str, Any]:
        codegraph = ROOT / "target" / "release" / "codegraph-mcp.exe"
        repo = self._prepare_repo("codegraph_context")
        db = self.output_dir / "db" / "codegraph_shared" / f"{self.instance_id}.sqlite"
        vector = self.output_dir / "db" / "codegraph_shared" / f"{self.instance_id}.vectors.json"
        spool = self.output_dir / "db" / "codegraph_shared" / f"{self.instance_id}.candidate_spool.jsonl"
        query_index = _candidate_query_index_path(spool)
        build_vector = "codegraph_full" in self.modes
        db.parent.mkdir(parents=True, exist_ok=True)
        index = [
            str(codegraph),
            "index",
            str(repo),
            "--db",
            str(db),
            "--fresh",
            "--json",
            "--profile",
            "--candidate-spool",
            str(spool),
            "--candidate-spool-policy",
            "bounded",
            "--candidate-spool-query-index",
            "yes",
        ]
        if build_vector:
            index.extend(["--build-vector-index", str(vector)])
        index_record = self._run_command(index, "codegraph_prebuild_index", timeout_s=self.codegraph_context_timeout_s)
        status_record = self._run_command([str(codegraph), "--repo", str(repo), "--db", str(db), "status", "--json"], "codegraph_prebuild_status", timeout_s=120)
        progress = _index_progress_summary(index_record.get("stderr", ""))
        status_json = _parse_json_loose(status_record.get("stdout", ""))
        manifest = {
            "schema_version": "swebench_codegraph_prebuild_v1",
            "task_id": self.instance_id,
            "repo_path": str(repo),
            "db_path": str(db),
            "vector_index": str(vector) if build_vector else None,
            "candidate_spool_path": str(spool),
            "candidate_spool_query_index_path": str(query_index),
            "build_vector": build_vector,
            "status": _prebuild_status(index_record, db),
            "index_command": _command_brief(index_record),
            "status_command": _command_brief(status_record),
            "db_exists": db.exists(),
            "db_bytes": db.stat().st_size if db.exists() else 0,
            "vector_exists": vector.exists() if build_vector else False,
            "vector_bytes": vector.stat().st_size if build_vector and vector.exists() else 0,
            "candidate_spool_exists": spool.exists(),
            "candidate_spool_bytes": spool.stat().st_size if spool.exists() else 0,
            "candidate_spool_query_index_exists": query_index.exists(),
            "candidate_spool_query_index_bytes": query_index.stat().st_size if query_index.exists() else 0,
            "index_progress": progress,
            "status_json": status_json,
        }
        if manifest["status"] != "complete" and manifest["candidate_spool_exists"]:
            manifest["staged_candidate_context_available"] = True
        else:
            manifest["staged_candidate_context_available"] = False
        self._write_json(self.output_dir / "codegraph_prebuild_manifest.json", manifest)
        self._write_json(self.output_dir / "index_progress_summary.json", progress)
        return manifest

    def _codegraph_context(
        self,
        task: dict[str, Any],
        repo: Path,
        mode: str,
        *,
        full: bool,
        prebuild: dict[str, Any] | None,
    ) -> dict[str, Any]:
        if not prebuild:
            return self._context_packet(
                mode,
                text="CodeGraph prebuild did not run.",
                files=[],
                tool_calls=0,
                wall_time_ms=0,
                codegraph_calls=0,
                context_valid_for_attribution=False,
                context_failure="codegraph_prebuild_missing",
                claimability={"graph_proof": False, "proof_status": "no_codegraph_context"},
            )
        codegraph = ROOT / "target" / "release" / "codegraph-mcp.exe"
        db = Path(prebuild["db_path"])
        vector = Path(prebuild["vector_index"]) if prebuild.get("vector_index") else None
        spool = Path(prebuild["candidate_spool_path"])
        if prebuild.get("status") != "complete":
            return self._staged_candidate_context(task, repo, mode, prebuild)
        if full and (not vector or not vector.exists()):
            return self._context_packet(
                mode,
                text=json.dumps(prebuild, indent=2),
                files=[],
                tool_calls=0,
                wall_time_ms=0,
                codegraph_calls=0,
                context_valid_for_attribution=False,
                context_failure="vector_sidecar_missing_after_prebuild",
                claimability={"graph_proof": False, "proof_status": "codegraph_full_unavailable"},
                db_path=str(db),
                vector_index=str(vector) if vector else None,
                codegraph_prebuild=prebuild,
            )
        records = [self._run_command([str(codegraph), "--repo", str(repo), "--db", str(db), "status", "--json"], f"{mode}_status", timeout_s=120)]
        context_cmd = [
            str(codegraph),
            "--repo",
            str(repo),
            "--db",
            str(db),
            "context-pack",
            "--task",
            task["task"],
            "--agent-json",
            "--max-output-bytes",
            str(MAX_CONTEXT_BYTES),
        ]
        if full and vector:
            context_cmd.extend(["--enable-vector-candidates", "--vector-index", str(vector), "--enable-nuance-rescue-candidates"])
        records.append(self._run_command(context_cmd, f"{mode}_context_pack", timeout_s=300))
        for kind, query in (("text", "__dict__"), ("text", "__slots__"), ("symbols", "Symbol"), ("symbols", "Printable"), ("files", "_print_helpers")):
            records.append(
                self._run_command(
                    [str(codegraph), "--repo", str(repo), "--db", str(db), "query", kind, query, "--agent-json", "--limit", "8"],
                    f"{mode}_query_{kind}_{_safe_slug(query)}",
                    timeout_s=120,
                )
            )
        files: list[str] = []
        for record in records[1:]:
            parsed = _parse_json_loose(record["stdout"])
            for path in _extract_files(parsed):
                if path not in files:
                    files.append(path)
        failed = [record for record in records if record["exit_code"] != 0]
        text = _trim_context(records[1:], MAX_CONTEXT_BYTES)
        return self._context_packet(
            mode,
            text=text,
            files=files[:12],
            tool_calls=len(records),
            wall_time_ms=sum(int(record["wall_time_ms"]) for record in records),
            codegraph_calls=len(records),
            context_valid_for_attribution=not failed and bool(files),
            context_failure="; ".join(record["failure_kind"] for record in failed) if failed else "",
            claimability={"graph_proof": False, "proof_status": "source_navigation_or_text_evidence"},
            db_path=str(db),
            vector_index=str(vector) if full and vector else None,
            codegraph_prebuild=prebuild,
        )

    def _staged_candidate_context(self, task: dict[str, Any], repo: Path, mode: str, prebuild: dict[str, Any]) -> dict[str, Any]:
        codegraph = ROOT / "target" / "release" / "codegraph-mcp.exe"
        db = Path(prebuild["db_path"])
        spool = Path(prebuild["candidate_spool_path"])
        if not spool.exists():
            return self._context_packet(
                mode,
                text=json.dumps(prebuild, indent=2),
                files=[],
                tool_calls=0,
                wall_time_ms=0,
                codegraph_calls=0,
                context_valid_for_attribution=False,
                context_failure=f"{prebuild.get('status', 'prebuild_failed')}; candidate_spool_missing",
                claimability={"graph_proof": False, "proof_status": "candidate_context_unavailable"},
                db_path=str(db),
                candidate_spool_path=str(spool),
                codegraph_prebuild=prebuild,
            )
        records = [
            self._run_command(
                [
                    str(codegraph),
                    "--repo",
                    str(repo),
                    "--db",
                    str(db),
                    "context-pack",
                    "--task",
                    task["task"],
                    "--candidate-spool",
                    str(spool),
                    "--early-candidates",
                    "--agent-json",
                    "--max-output-bytes",
                    str(MAX_CONTEXT_BYTES),
                ],
                f"{mode}_staged_candidate_context_pack",
                timeout_s=120,
            )
        ]
        for kind, query in (("files", "_print_helpers"), ("symbols", "Printable"), ("symbols", "Symbol"), ("text", "__dict__"), ("text", "__slots__")):
            records.append(
                self._run_command(
                    [
                        str(codegraph),
                        "--repo",
                        str(repo),
                        "--db",
                        str(db),
                        "query",
                        kind,
                        query,
                        "--candidate-spool",
                        str(spool),
                        "--early-candidates",
                        "--agent-json",
                        "--limit",
                        "8",
                    ],
                    f"{mode}_staged_candidate_query_{kind}_{_safe_slug(query)}",
                    timeout_s=120,
                )
            )
        files: list[str] = []
        for record in records:
            parsed = _parse_json_loose(record["stdout"])
            for path in _extract_files(parsed):
                if path not in files:
                    files.append(path)
        text = _trim_context(records, MAX_CONTEXT_BYTES)
        useful = _candidate_context_has_gold_hit(task, files, text)
        failed = [record for record in records if record["exit_code"] != 0]
        if not useful:
            failure = f"{prebuild.get('status', 'prebuild_failed')}; candidate_spool_present_but_no_gold_hit"
            if failed:
                failure += "; " + "; ".join(record["failure_kind"] for record in failed)
        else:
            failure = f"{prebuild.get('status', 'prebuild_failed')}; staged_candidate_context_used"
        return self._context_packet(
            mode,
            text=text,
            files=files[:12],
            tool_calls=len(records),
            wall_time_ms=sum(int(record["wall_time_ms"]) for record in records),
            codegraph_calls=len(records),
            context_valid_for_attribution=useful and not failed,
            context_failure=failure,
            claimability={
                "graph_proof": False,
                "proof_status": "candidate_only_until_verified" if useful else "candidate_context_unavailable",
                "candidate_only": True,
                "source_navigation_or_graph_verification": False,
            },
            db_path=str(db),
            candidate_spool_path=str(spool),
            candidate_only=True,
            codegraph_prebuild=prebuild,
        )

    def _context_packet(self, mode: str, *, text: str, files: list[str], tool_calls: int, wall_time_ms: int, rg_calls: int = 0, codegraph_calls: int = 0, context_valid_for_attribution: bool = True, context_failure: str = "", claimability: dict[str, Any] | None = None, **extra: Any) -> dict[str, Any]:
        raw_bytes = len(text.encode("utf-8", errors="ignore"))
        packet = {
            "provider": mode,
            "files": files,
            "snippets_text": text,
            "raw_context_bytes": raw_bytes,
            "estimated_context_tokens": raw_bytes // 4,
            "tool_calls": tool_calls,
            "rg_calls": rg_calls,
            "codegraph_calls": codegraph_calls,
            "wall_time_ms": wall_time_ms,
            "context_valid_for_attribution": context_valid_for_attribution,
            "context_failure": context_failure,
            "claimability": claimability or {"graph_proof": False, "proof_status": "no_extra_context"},
            "graph_proof": False,
            "claim_boundary": "context packet is evidence/routing input, not graph proof",
        }
        packet.update(extra)
        return packet

    def _run_external_agent(self, task: dict[str, Any], context: dict[str, Any], repo: Path, mode: str) -> dict[str, Any]:
        if self.skip_agent:
            return {"status": "skipped_by_request", "patch_text": None, "blocked_reason": "agent execution skipped by --skip-agent."}
        if not self.external_agent_command:
            return {"status": "blocked_not_configured", "patch_text": None, "blocked_reason": "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND is not configured."}
        payload = {
            "task": {
                "task_id": self.instance_id,
                "instance_id": self.instance_id,
                "repo": task["repo"],
                "base_commit": task["base_commit"],
                "workspace_path": str(repo),
                "task": task["task"],
            },
            "context": context,
            "budgets": {
                "max_time_s": MAX_TIME_S,
                "max_tool_calls": MAX_TOOL_CALLS,
                "max_context_bytes": MAX_CONTEXT_BYTES,
                "max_output_bytes": MAX_CONTEXT_BYTES,
            },
        }
        payload_path = self.output_dir / "per_mode" / mode / "agent_payload.json"
        self._write_json(payload_path, payload)
        command = _split_windows_command(self.external_agent_command)
        started = time.perf_counter()
        guarded = run_command_with_resource_guard(
            command,
            input_text=json.dumps(payload),
            cwd=ROOT,
            timeout_s=self.agent_timeout_s,
            resource_limits=self.resource_limits,
        )
        wall_ms = int((time.perf_counter() - started) * 1000)
        stdout = guarded.stdout.decode("utf-8", errors="replace")
        stderr = guarded.stderr.decode("utf-8", errors="replace")
        (self.logs_dir / f"{mode}_agent_stdout.patch_or_text").write_text(stdout, encoding="utf-8")
        (self.logs_dir / f"{mode}_agent_stderr.txt").write_text(stderr, encoding="utf-8")
        agent_record = self._append_command_record(
            command_id=f"{mode}_external_agent",
            command=command,
            cwd=ROOT,
            timeout_s=self.agent_timeout_s,
            exit_code=guarded.exit_code,
            stdout=stdout,
            stderr=stderr,
            wall_time_ms=wall_ms,
            failure_kind=guarded.failure_kind,
            exception=guarded.exception,
            resource_guard=guarded.resource_guard,
            stdout_path=self.logs_dir / f"{mode}_agent_stdout.patch_or_text",
            stderr_path=self.logs_dir / f"{mode}_agent_stderr.txt",
        )
        patch_text = stdout if stdout.lstrip().startswith("diff --git") else ""
        if not patch_text:
            salvage = self._run_command(["git", "diff", "--no-ext-diff", "--binary", "--"], f"{mode}_salvage_git_diff", cwd=repo, timeout_s=60)
            if salvage["stdout"].lstrip().startswith("diff --git"):
                patch_text = salvage["stdout"]
        return {
            "status": "ok" if guarded.exit_code == 0 and patch_text else "failed",
            "exit_code": guarded.exit_code,
            "wall_time_ms": wall_ms,
            "stdout_bytes": len(stdout.encode("utf-8", errors="ignore")),
            "stderr_bytes": len(stderr.encode("utf-8", errors="ignore")),
            "stderr_tail": stderr[-4000:],
            "patch_text": patch_text,
            "patch_capture": "wrapper_stdout" if stdout.lstrip().startswith("diff --git") else ("salvaged_git_diff" if patch_text else "none"),
            "failure_kind": agent_record["failure_kind"],
            "resource_guard": agent_record.get("resource_guard"),
        }

    def _run_swebench_eval(self, prediction_paths: dict[str, Path]) -> dict[str, Any]:
        if not prediction_paths:
            return {}
        script_lines = [
            "set -u",
            "cd /work",
            "apt-get update >/dev/null",
            "DEBIAN_FRONTEND=noninteractive apt-get install -y python3-pip python3-venv git >/dev/null",
            "python3 -m pip install --break-system-packages -e /work/benchmarks/upstream/SWE-bench >/dev/null",
        ]
        for mode, pred in prediction_paths.items():
            mode_dir = self.eval_dir / mode
            mode_dir.mkdir(parents=True, exist_ok=True)
            rel_pred = pred.relative_to(ROOT).as_posix()
            rel_mode_dir = mode_dir.relative_to(ROOT).as_posix()
            run_id = f"{self.output_dir.name}-{mode}"
            script_lines.extend(
                [
                    f"cd /work/{rel_mode_dir}",
                    "set +e",
                    (
                        "python3 -m swebench.harness.run_evaluation "
                        "--dataset_name princeton-nlp/SWE-bench_Lite "
                        f"--predictions_path /work/{rel_pred} "
                        "--max_workers 1 "
                        f"--instance_ids {self.instance_id} "
                        f"--run_id {run_id} "
                        f"--timeout {MAX_TIME_S} "
                        f"--report_dir /work/{rel_mode_dir}"
                    ),
                    "echo $? > exit_code.txt",
                    "set -u",
                ]
            )
        docker_script = "\n".join(script_lines)
        record = self._run_command(
            [
                "docker",
                "run",
                "--rm",
                "-v",
                "/var/run/docker.sock:/var/run/docker.sock",
                "-v",
                f"{ROOT}:/work",
                "-w",
                "/work",
                "node:20-bookworm",
                "bash",
                "-lc",
                docker_script,
            ],
            "swebench_eval_all_modes",
            timeout_s=max(2400, MAX_TIME_S * max(1, len(prediction_paths))),
        )
        results: dict[str, Any] = {}
        for mode in prediction_paths:
            results[mode] = self._load_eval_result(mode, record)
        return results

    def _load_eval_result(self, mode: str, docker_record: dict[str, Any]) -> dict[str, Any]:
        mode_dir = self.eval_dir / mode
        report_candidates = list(mode_dir.glob(f"codex_cli_{mode}.*.json"))
        detail_candidates = list(mode_dir.glob(f"logs/run_evaluation/*/codex_cli_{mode}/{self.instance_id}/report.json"))
        exit_code_path = mode_dir / "exit_code.txt"
        summary = _read_json(report_candidates[0]) if report_candidates else {}
        detail = _read_json(detail_candidates[0]) if detail_candidates else {}
        instance_detail = detail.get(self.instance_id, {}) if isinstance(detail, dict) else {}
        return {
            "status": "completed" if report_candidates or detail_candidates else "missing_report",
            "docker_command_exit_code": docker_record["exit_code"],
            "harness_exit_code": exit_code_path.read_text(encoding="utf-8").strip() if exit_code_path.exists() else None,
            "summary_path": str(report_candidates[0]) if report_candidates else None,
            "detail_path": str(detail_candidates[0]) if detail_candidates else None,
            "completed": bool(summary.get("completed_instances") or instance_detail),
            "resolved": instance_detail.get("resolved"),
            "tests_passed": _count_success_tests(instance_detail),
            "raw_summary": summary,
        }

    def _run_command(self, command: list[str], command_id: str, *, cwd: Path | None = None, timeout_s: int = 120) -> dict[str, Any]:
        cwd = cwd or ROOT
        started = time.perf_counter()
        wall_time_ms = int((time.perf_counter() - started) * 1000)
        stdout_path = self.logs_dir / f"{command_id}.stdout.txt"
        stderr_path = self.logs_dir / f"{command_id}.stderr.txt"
        try:
            guarded = run_command_with_resource_guard(
                command,
                cwd=cwd,
                timeout_s=timeout_s,
                resource_limits=self.resource_limits,
            )
            stdout = guarded.stdout.decode("utf-8", errors="replace")
            stderr = guarded.stderr.decode("utf-8", errors="replace")
            exit_code = guarded.exit_code
            failure_kind = guarded.failure_kind or ("none" if exit_code == 0 else f"exit_{exit_code}")
            exception = guarded.exception
            resource_guard = guarded.resource_guard
        except FileNotFoundError as exc:
            stdout = ""
            stderr = str(exc)
            exit_code = None
            failure_kind = "missing_binary"
            exception = f"FileNotFoundError: {exc}"
            resource_guard = {"enabled": self.resource_limits.enabled}
        wall_time_ms = int((time.perf_counter() - started) * 1000)
        stdout_path.write_text(stdout, encoding="utf-8", errors="ignore")
        stderr_path.write_text(stderr, encoding="utf-8", errors="ignore")
        return self._append_command_record(
            command_id=command_id,
            command=command,
            cwd=cwd,
            timeout_s=timeout_s,
            exit_code=exit_code,
            stdout=stdout,
            stderr=stderr,
            wall_time_ms=wall_time_ms,
            failure_kind=failure_kind,
            exception=exception,
            resource_guard=resource_guard,
            stdout_path=stdout_path,
            stderr_path=stderr_path,
        )

    def _append_command_record(
        self,
        *,
        command_id: str,
        command: list[str],
        cwd: Path,
        timeout_s: int,
        exit_code: int | None,
        stdout: str,
        stderr: str,
        wall_time_ms: int,
        failure_kind: str | None,
        exception: str | None,
        resource_guard: dict,
        stdout_path: Path,
        stderr_path: Path,
    ) -> dict[str, Any]:
        record = {
            "command_id": command_id,
            "argv": command,
            "cwd": str(cwd),
            "timeout_s": timeout_s,
            "exit_code": exit_code,
            "success": exit_code == 0 and exception is None,
            "failure_kind": failure_kind,
            "exception": exception,
            "wall_time_ms": wall_time_ms,
            "stdout_path": str(stdout_path),
            "stderr_path": str(stderr_path),
            "stdout": stdout,
            "stderr": stderr,
            "stdout_bytes": len(stdout.encode("utf-8", errors="ignore")),
            "stderr_bytes": len(stderr.encode("utf-8", errors="ignore")),
            "resource_guard": resource_guard,
        }
        with self.commands_jsonl.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps({k: v for k, v in record.items() if k not in {"stdout", "stderr"}}) + "\n")
        return record

    def _must(self, record: dict[str, Any]) -> dict[str, Any]:
        if not record.get("success"):
            raise RuntimeError(
                f"command failed: {record.get('command_id')} exit={record.get('exit_code')} stderr={record.get('stderr_path')}"
            )
        return record

    def _load_task(self) -> dict[str, Any]:
        task = self._load_task_fixture()
        if task:
            return task
        try:
            import pyarrow.ipc as ipc
        except ModuleNotFoundError as exc:
            raise BenchmarkBlocked(
                "blocked_missing_pyarrow: install pyarrow or provide --task-fixture with the SWE-bench task JSON."
            ) from exc
        arrow = self._swebench_arrow_path()
        table = ipc.open_stream(str(arrow)).read_all()
        row = next(item for item in table.to_pylist() if item["instance_id"] == self.instance_id)
        return {
            "task_id": self.instance_id,
            "instance_id": self.instance_id,
            "repo": row["repo"],
            "base_commit": row["base_commit"],
            "task": row["problem_statement"],
            "problem_statement": row["problem_statement"],
            "gold_patch": row["patch"],
            "test_patch": row["test_patch"],
            "gold_files": changed_files_from_patch(row["patch"]),
            "gold_symbols": ["Printable", "Symbol", "Basic", "__slots__", "__dict__"],
        }

    def _load_task_fixture(self) -> dict[str, Any] | None:
        for path in (self.task_fixture, LEGACY_TASK_CACHE):
            if not path or not path.exists():
                continue
            data = _read_json(path)
            if data.get("instance_id") != self.instance_id and data.get("task_id") != self.instance_id:
                continue
            return _normalize_task(data)
        return None

    def _swebench_arrow_path(self) -> Path:
        cache = Path.home() / ".cache" / "huggingface" / "datasets" / "princeton-nlp___swe-bench_lite"
        candidates = sorted(cache.rglob("swe-bench_lite-test.arrow"))
        if not candidates:
            raise FileNotFoundError("cached SWE-bench Lite test arrow was not found")
        return candidates[-1]

    def _default_source_repo(self) -> Path:
        prior = ROOT / "benchmarks" / "results" / "summaries" / "swebench_focused_20260521_163620" / "repos" / "sympy-source"
        if not prior.exists():
            prior = ROOT / "benchmarks" / "results" / "summaries" / "swebench_focused_20260521_163620" / "repos" / f"{self.instance_id}_rg_strong"
        return prior

    def _run_plan(self, task: dict[str, Any]) -> dict[str, Any]:
        return {
            "schema_version": "swebench_patch_smoke_run_plan_v1",
            "run_id": self.output_dir.name,
            "task_id": self.instance_id,
            "modes": self.modes,
            "task_repo": task["repo"],
            "base_commit": task["base_commit"],
            "source_repo": str(self.source_repo),
            "task_fixture": str(self.task_fixture) if self.task_fixture else "",
            "external_agent_command": "configured" if self.external_agent_command else "not_configured",
            "skip_agent": self.skip_agent,
            "skip_eval": self.skip_eval,
            "budgets": {
                "max_time_s": MAX_TIME_S,
                "codegraph_context_timeout_s": self.codegraph_context_timeout_s,
                "agent_timeout_s": self.agent_timeout_s,
                "max_tool_calls": MAX_TOOL_CALLS,
                "max_context_bytes": MAX_CONTEXT_BYTES,
            },
            "resource_guard": self.resource_limits.to_dict(),
            "claim_boundary": "local diagnostic only; no official SWE-bench score claim",
        }

    def _write_json(self, path: Path, data: Any) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    def _write_summary_md(self, summary: dict[str, Any]) -> None:
        lines = [
            f"# SWE-bench Lite Patch Smoke: {summary['task_id']}",
            "",
            "Local one-task diagnostic only. This is not an official SWE-bench score or public benchmark claim.",
            "",
            "| Mode | Patch | Resolved | Wrong-file edits | Context bytes | Context valid |",
            "| --- | ---: | ---: | ---: | ---: | ---: |",
        ]
        for mode, result in summary["results_by_mode"].items():
            swe = result.get("swebench", {})
            lines.append(
                f"| `{mode}` | {bool(result.get('patch_applied_static'))} | {swe.get('resolved')} | "
                f"{result.get('wrong_file_edits')} | {result.get('context_bytes')} | {result.get('context_valid_for_attribution')} |"
            )
        lines.extend(
            [
                "",
                "| Mode | Clean source patch | Reason | Source edits | Test edits | Extra edits |",
                "| --- | ---: | --- | ---: | ---: | ---: |",
            ]
        )
        for mode, result in summary["results_by_mode"].items():
            lines.append(
                f"| `{mode}` | {result.get('clean_source_patch')} | {result.get('clean_source_patch_reason')} | "
                f"{len(result.get('source_file_edits') or [])} | {len(result.get('test_file_edits') or [])} | "
                f"{len(result.get('extra_file_edits') or [])} |"
            )
        lines.append("")
        lines.append("Graph/vector/source-navigation/context output is evidence for the agent, not graph proof.")
        (self.output_dir / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def _split_windows_command(value: str) -> list[str]:
    import shlex

    return shlex.split(value, posix=False)


def _safe_slug(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "_", value)


def _mode_repo_slug(mode: str) -> str:
    aliases = {
        "baseline": "b",
        "rg_only": "rg",
        "codegraph_exact_text": "cg_text",
        "codegraph_full": "cg_full",
        "codegraph_context": "cg_context",
    }
    return aliases.get(mode, _safe_slug(mode)[:24] or "repo")


def _candidate_query_index_path(spool: Path) -> Path:
    return spool.with_suffix(".query.sqlite")


def _prebuild_status(index_record: dict[str, Any], db: Path) -> str:
    if index_record.get("success") and db.exists():
        return "complete"
    failure = index_record.get("failure_kind") or "failed"
    if failure == "timeout":
        return "blocked_index_timeout"
    if failure == "resource_limit":
        return "blocked_resource_limit"
    if failure == "missing_binary":
        return "blocked_missing_binary"
    return "failed"


def _command_brief(record: dict[str, Any]) -> dict[str, Any]:
    return {
        "command_id": record.get("command_id"),
        "argv": record.get("argv"),
        "timeout_s": record.get("timeout_s"),
        "exit_code": record.get("exit_code"),
        "success": record.get("success"),
        "failure_kind": record.get("failure_kind"),
        "exception": record.get("exception"),
        "wall_time_ms": record.get("wall_time_ms"),
        "stdout_path": record.get("stdout_path"),
        "stderr_path": record.get("stderr_path"),
        "stdout_bytes": record.get("stdout_bytes"),
        "stderr_bytes": record.get("stderr_bytes"),
        "resource_guard": record.get("resource_guard"),
    }


def _index_progress_summary(stderr: str) -> dict[str, Any]:
    summary: dict[str, Any] = {
        "profile_available": False,
        "event_count": 0,
        "files_started": 0,
        "files_completed": 0,
        "last_event": None,
        "last_stage": None,
        "last_file": None,
        "recent_files": [],
        "issue_count": 0,
    }
    recent_files: list[str] = []
    for line in stderr.splitlines():
        event = _parse_json_loose(line.strip())
        if not isinstance(event, dict):
            continue
        summary["profile_available"] = True
        summary["event_count"] += 1
        event_name = str(event.get("event") or event.get("kind") or event.get("stage") or "")
        summary["last_event"] = event_name or None
        stage = event.get("stage") or event.get("phase")
        if stage:
            summary["last_stage"] = str(stage)
        file_value = event.get("file") or event.get("path") or event.get("repo_relative_path")
        if file_value:
            file_text = str(file_value).replace("\\", "/")
            summary["last_file"] = file_text
            recent_files.append(file_text)
        if event_name == "file_extract_started":
            summary["files_started"] += 1
        if event_name == "file_extract_completed":
            summary["files_completed"] += 1
        if "issue" in event_name or "error" in event_name:
            summary["issue_count"] += 1
    summary["recent_files"] = recent_files[-10:]
    return summary


def _candidate_context_has_gold_hit(task: dict[str, Any], files: list[str], text: str) -> bool:
    normalized_files = {path.replace("\\", "/").lstrip("./") for path in files}
    normalized_text = text.replace("\\", "/")
    for gold in task.get("gold_files") or []:
        gold_text = str(gold).replace("\\", "/").lstrip("./")
        if gold_text in normalized_files or gold_text in normalized_text:
            return True
    return False


def _patch_file_quality(patch_text: str, gold_files: list[str]) -> dict[str, Any]:
    changed = changed_files_from_patch(patch_text)
    allowed = {path.replace("\\", "/").lstrip("./") for path in gold_files}
    test_file_edits = [path for path in changed if _is_test_path(path)]
    source_file_edits = [path for path in changed if not _is_test_path(path)]
    extra_file_edits = [path for path in changed if path.replace("\\", "/").lstrip("./") not in allowed]
    return {
        "changed_files": changed,
        "wrong_file_edits": wrong_file_edits(patch_text, gold_files) if patch_text else None,
        "test_file_edits": test_file_edits,
        "source_file_edits": source_file_edits,
        "extra_file_edits": extra_file_edits,
    }


def _is_test_path(path: str) -> bool:
    normalized = path.replace("\\", "/").lstrip("./").lower()
    name = normalized.rsplit("/", 1)[-1]
    parts = normalized.split("/")
    return (
        "tests" in parts
        or normalized.startswith("test/")
        or normalized.startswith("tests/")
        or name.startswith("test_")
        or name.endswith("_test.py")
        or name.endswith(".test")
    )


def _files_from_rg(text: str) -> list[str]:
    files: list[str] = []
    for line in text.splitlines():
        if not line.strip():
            continue
        path = line.split(":", 1)[0].replace("\\", "/").lstrip("./")
        if path and path not in files and not path.startswith("--"):
            files.append(path)
    return files


def _score_rg_path(path: str, hits: dict[str, int]) -> tuple[int, str]:
    score = hits.get(path, 0)
    name = path.lower()
    for token in ("symbol", "print", "basic", "slots", "dict"):
        if token in name:
            score += 3
    if name.startswith("sympy/core/"):
        score += 2
    if "test" in name:
        score -= 1
    return (-score, path)


def _trim_context(records: list[dict[str, Any]], limit: int) -> str:
    chunks: list[str] = []
    used = 0
    for record in records:
        chunk = "\n\n$ " + " ".join(str(part) for part in record["argv"]) + "\n" + (record.get("stdout") or "")[:30_000]
        chunk_bytes = len(chunk.encode("utf-8", errors="ignore"))
        if used + chunk_bytes > limit:
            remain = max(0, limit - used)
            chunks.append(chunk.encode("utf-8", errors="ignore")[:remain].decode("utf-8", errors="ignore"))
            break
        chunks.append(chunk)
        used += chunk_bytes
    return "".join(chunks)


def _parse_json_loose(text: str) -> Any | None:
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        start = text.find("{")
        end = text.rfind("}")
        if start >= 0 and end > start:
            try:
                return json.loads(text[start : end + 1])
            except json.JSONDecodeError:
                return None
    return None


def _extract_files(value: Any) -> list[str]:
    files: list[str] = []

    def add(path: Any) -> None:
        if path is None:
            return
        text = str(path).replace("\\", "/").lstrip("./")
        if text and text not in files:
            files.append(text)

    def walk(item: Any) -> None:
        if isinstance(item, dict):
            for key in ("file", "path", "file_path", "repo_relative_path"):
                if key in item:
                    add(item[key])
            for child in item.values():
                if isinstance(child, (dict, list)):
                    walk(child)
        elif isinstance(item, list):
            for child in item:
                if isinstance(child, str) and ("/" in child or "\\" in child):
                    add(child)
                else:
                    walk(child)

    walk(value)
    return files


def _read_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}


def _normalize_task(data: dict[str, Any]) -> dict[str, Any]:
    gold_patch = data.get("gold_patch") or data.get("patch") or ""
    task_text = data.get("task") or data.get("problem_statement") or ""
    return {
        "task_id": data.get("task_id") or data.get("instance_id") or TASK_ID,
        "instance_id": data.get("instance_id") or data.get("task_id") or TASK_ID,
        "repo": data.get("repo") or "sympy/sympy",
        "base_commit": data.get("base_commit") or BASE_COMMIT,
        "task": task_text,
        "problem_statement": data.get("problem_statement") or task_text,
        "gold_patch": gold_patch,
        "test_patch": data.get("test_patch") or "",
        "gold_files": data.get("gold_files") or changed_files_from_patch(gold_patch),
        "gold_symbols": data.get("gold_symbols") or ["Printable", "Symbol", "Basic", "__slots__", "__dict__"],
    }


def _load_jsonl(path: Path) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    records = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            records.append(json.loads(line))
    return records


def _count_success_tests(instance_detail: dict[str, Any]) -> int | None:
    tests_status = instance_detail.get("tests_status")
    if not isinstance(tests_status, dict):
        return None
    count = 0
    for group in tests_status.values():
        if isinstance(group, dict):
            success = group.get("success")
            if isinstance(success, list):
                count += len(success)
    return count


def _bytes_or_text(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="ignore")
    return str(value)


if __name__ == "__main__":
    raise SystemExit(main())
