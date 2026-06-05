from __future__ import annotations

import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmarks.harness.resource_guard import ResourceLimits
from benchmarks.harness.runners.run_swebench_patch_smoke import (
    BASE_COMMIT,
    BenchmarkBlocked,
    CODEGRAPH_PREBUILD_SCOPE,
    PatchSmokeRunner,
    TASK_ID,
    _agent_visible_context,
    _classify_codegraph_gold_miss,
    _candidate_context_has_gold_hit,
    _candidate_query_index_path,
    _codegraph_context_claimability,
    _context_gold_diagnostic,
    _index_progress_summary,
    _is_test_path,
    _patch_file_quality,
    _phase_gates,
    _prebuild_status,
    _provider_visible_query_diagnostics,
    _provider_visible_query_specs,
    _rank_provider_visible_scope_files,
    main,
)


class SwebenchPatchSmokeRunnerTests(unittest.TestCase):
    def _runner(self, output_dir: Path, *, instance_id: str = TASK_ID, task_fixture: Path | None) -> PatchSmokeRunner:
        return PatchSmokeRunner(
            output_dir=output_dir,
            instance_id=instance_id,
            modes=[],
            external_agent_command="",
            source_repo=output_dir / "source_repo",
            skip_eval=True,
            skip_agent=True,
            codegraph_context_timeout_s=1,
            codegraph_prebuild_scope=CODEGRAPH_PREBUILD_SCOPE,
            agent_timeout_s=1,
            task_fixture=task_fixture,
            resource_limits=ResourceLimits(enabled=False),
        )

    def test_task_fixture_loads_without_pyarrow_dependency(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fixture = root / "task.json"
            fixture.write_text(
                json.dumps(
                    {
                        "instance_id": TASK_ID,
                        "repo": "sympy/sympy",
                        "base_commit": BASE_COMMIT,
                        "problem_statement": "fixture task",
                        "patch": (
                            "diff --git a/sympy/core/_print_helpers.py b/sympy/core/_print_helpers.py\n"
                            "--- a/sympy/core/_print_helpers.py\n"
                            "+++ b/sympy/core/_print_helpers.py\n"
                            "@@ -1 +1 @@\n"
                            "-old\n"
                            "+new\n"
                        ),
                        "test_patch": "diff --git a/test.py b/test.py\n",
                    }
                ),
                encoding="utf-8",
            )

            task = self._runner(root / "out", task_fixture=fixture)._load_task()

        self.assertEqual(task["instance_id"], TASK_ID)
        self.assertEqual(task["repo"], "sympy/sympy")
        self.assertEqual(task["task"], "fixture task")
        self.assertEqual(task["gold_files"], ["sympy/core/_print_helpers.py"])

    def test_prepare_repo_uses_task_base_commit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            runner = self._runner(root / "out", task_fixture=None)
            runner.source_repo.mkdir(parents=True)
            commands: list[tuple[str, list[str]]] = []

            def fake_run(command, command_id, *, cwd=None, timeout_s=120):  # type: ignore[no-untyped-def]
                commands.append((command_id, command))
                return {
                    "argv": command,
                    "command_id": command_id,
                    "stdout": "",
                    "stderr": "",
                    "exit_code": 0,
                    "success": True,
                    "failure_kind": "none",
                    "wall_time_ms": 1,
                }

            runner._run_command = fake_run  # type: ignore[method-assign]

            runner._prepare_repo("rg_only", {"base_commit": "task-specific-commit"})

        checkout = [command for command_id, command in commands if command_id == "checkout_rg_only"][0]
        reset = [command for command_id, command in commands if command_id == "reset_rg_only"][0]
        self.assertIn("task-specific-commit", checkout)
        self.assertIn("task-specific-commit", reset)
        self.assertNotIn(BASE_COMMIT, checkout)
        self.assertNotIn(BASE_COMMIT, reset)

    def test_cli_uses_canonical_config_external_agent_when_env_absent(self) -> None:
        captured: dict[str, object] = {}

        def fake_init(self, **kwargs):  # type: ignore[no-untyped-def]
            captured.update(kwargs)

        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True), patch.object(
            PatchSmokeRunner, "__init__", fake_init
        ), patch.object(PatchSmokeRunner, "run", lambda self: {"status": "complete"}):
            code = main(["--output-dir", str(Path(tmp) / "out"), "--skip-agent", "--skip-eval"])

        self.assertEqual(code, 0)
        self.assertEqual(
            captured["external_agent_command"],
            "powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/tracks/swebench_lite/scripts/run_codex_external_patch_agent.ps1",
        )

    def test_missing_fixture_reports_pyarrow_blocker_when_pyarrow_is_absent(self) -> None:
        if importlib.util.find_spec("pyarrow") is not None:
            self.skipTest("pyarrow is installed in this environment")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            runner = self._runner(
                root / "out",
                instance_id="not_a_cached_fixture",
                task_fixture=root / "missing_task.json",
            )

            with self.assertRaises(BenchmarkBlocked) as cm:
                runner._load_task()

        self.assertIn("blocked_missing_pyarrow", str(cm.exception))

    def test_prebuild_status_labels_timeout_without_db(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            db = Path(tmp) / "missing.sqlite"
            status = _prebuild_status({"success": False, "failure_kind": "timeout"}, db)

        self.assertEqual(status, "blocked_index_timeout")

    def test_prebuild_status_labels_timeout_with_existing_db_as_blocked(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            db = Path(tmp) / "old-good.sqlite"
            db.write_text("old-good", encoding="utf-8")
            status = _prebuild_status({"success": False, "failure_kind": "timeout"}, db)

        self.assertEqual(status, "blocked_index_timeout")

    def test_candidate_query_index_path_matches_release_sidecar_name(self) -> None:
        spool = Path("sympy__sympy-20590.candidate_spool.jsonl")

        self.assertEqual(
            _candidate_query_index_path(spool).name,
            "sympy__sympy-20590.candidate_spool.jsonl.query.sqlite",
        )

    def test_provider_visible_query_specs_do_not_leak_hidden_gold_file(self) -> None:
        task = {
            "task": "sympy.Symbol('s').__dict__ changed but __slots__ should prevent it.",
            "gold_files": ["sympy/core/_print_helpers.py"],
            "gold_symbols": ["Printable", "Symbol", "__dict__", "__slots__"],
        }

        specs = _provider_visible_query_specs(task)
        terms = {spec["term"] for spec in specs}

        self.assertIn("Symbol", terms)
        self.assertIn("__dict__", terms)
        self.assertIn("__slots__", terms)
        self.assertNotIn("_print_helpers", terms)
        self.assertNotIn("Printable", terms)
        self.assertTrue(all("gold_overlap" not in spec for spec in specs))
        self.assertTrue(all(spec["allowed"] for spec in specs))

        diagnostics = _provider_visible_query_diagnostics(specs, task)
        self.assertTrue(all(item["gold_diagnostic_only"] for item in diagnostics))
        self.assertTrue(any(item["gold_overlap"] for item in diagnostics))

    def test_provider_visible_scope_ranking_prefers_core_visible_files_without_gold(self) -> None:
        hits = {
            "sympy/core/symbol.py": 2,
            "sympy/core/basic.py": 1,
            "sympy/core/_print_helpers.py": 1,
            "sympy/combinatorics/prufer.py": 3,
        }

        selected = _rank_provider_visible_scope_files(hits, limit=3)

        self.assertIn("sympy/core/symbol.py", selected)
        self.assertNotIn("sympy/core/_print_helpers.py", selected[:1])

    def test_agent_visible_context_strips_gold_diagnostics_before_model_payload(self) -> None:
        context = {
            "provider_visible_queries": [
                {"term": "Symbol", "gold_overlap": True, "visible_in_prompt": True},
            ],
            "gold_diagnostic": {"gold_files": ["sympy/core/_print_helpers.py"]},
            "codegraph_prebuild": {
                "status": "complete",
                "gold_extraction_diagnostic": {"gold_files": ["sympy/core/_print_helpers.py"]},
            },
        }

        visible = _agent_visible_context(context)
        text = json.dumps(visible)

        self.assertNotIn("gold", text.lower())
        self.assertEqual(visible["provider_visible_queries"][0]["term"], "Symbol")

    def test_codegraph_context_claimability_requires_claimable_graph_db(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            db = root / "db" / "cg.sqlite"
            repo.mkdir()
            db.parent.mkdir()
            db.write_text("", encoding="utf-8")

            claimable = _codegraph_context_claimability(
                {
                    "claimability": {"claimable": True, "graph_proof_available": True},
                    "graph_db_status": "ready",
                },
                db=db,
                repo=repo,
            )
            stale = _codegraph_context_claimability(
                {
                    "claimability": {"claimable": False, "graph_proof_available": True},
                    "graph_db_status": "repo_head_mismatch",
                },
                db=db,
                repo=repo,
            )

        self.assertTrue(claimable["db_claimable"])
        self.assertTrue(claimable["graph_proof_available"])
        self.assertTrue(claimable["external_db_used"])
        self.assertFalse(claimable["graph_proof"])
        self.assertFalse(stale["db_claimable"])
        self.assertFalse(stale["graph_proof_available"])

    def test_context_gold_diagnostic_is_diagnostic_only(self) -> None:
        task = {
            "gold_files": ["sympy/core/_print_helpers.py"],
            "gold_symbols": ["Printable"],
        }

        diagnostic = _context_gold_diagnostic(
            task,
            ["sympy/core/_print_helpers.py"],
            "class Printable: pass",
        )

        self.assertTrue(diagnostic["gold_diagnostic_only"])
        self.assertTrue(diagnostic["gold_not_passed_to_provider"])
        self.assertTrue(diagnostic["gold_returned_to_agent"])
        self.assertEqual(diagnostic["gold_file_hits_in_files"], ["sympy/core/_print_helpers.py"])

    def test_phase_gates_do_not_mark_codegraph_unmeasured_run_ready(self) -> None:
        mode_results = {
            "baseline": {"agent": {"status": "ok"}, "swebench": {"status": "completed"}},
            "codegraph_exact_text": {
                "context_valid_for_attribution": False,
                "agent": {"status": "skipped_invalid_context"},
                "swebench": {"status": "not_run"},
            },
        }

        gates = _phase_gates(["baseline", "codegraph_exact_text"], mode_results, {"count": 0, "commands": []})

        self.assertFalse(gates["phase_gate_ready"])
        self.assertTrue(gates["external_agent_working"])
        self.assertTrue(gates["swebench_eval_working"])
        self.assertFalse(gates["codegraph_patch_quality_measured"])
        self.assertIn("codegraph_patch_quality_not_measured", gates["phase_gate_blockers"])

    def test_phase_gates_separate_context_attribution_from_patch_quality(self) -> None:
        mode_results = {
            "codegraph_exact_text": {
                "context_valid_for_attribution": True,
                "agent": {"status": "skipped_by_request"},
                "swebench": {"status": "not_run"},
            },
        }

        gates = _phase_gates(["codegraph_exact_text"], mode_results, {"count": 0, "commands": []})

        self.assertTrue(gates["codegraph_context_attributable"])
        self.assertFalse(gates["codegraph_patch_quality_measured"])
        self.assertFalse(gates["phase_gate_ready"])

    def test_gold_miss_classifies_extracted_file_missing_from_spool(self) -> None:
        context = {
            "gold_files": ["sympy/core/_print_helpers.py"],
            "gold_returned_to_agent": False,
        }
        spool = {
            "gold_extraction_diagnostic": {
                "completed_gold_files": ["sympy/core/_print_helpers.py"],
            },
            "gold_files_in_spool_text": [],
            "gold_files_in_query_index": [],
        }

        classification = _classify_codegraph_gold_miss(
            prebuild={"status": "blocked_index_timeout"},
            spool_diagnostic=spool,
            context_gold_diagnostic=context,
        )

        self.assertEqual(classification, "extracted_gold_not_published_to_spool_before_timeout")

    def test_index_progress_summary_preserves_last_profile_file(self) -> None:
        stderr = "\n".join(
            [
                json.dumps({"event": "file_extract_started", "file": "sympy/core/basic.py"}),
                json.dumps({"event": "file_extract_completed", "file": "sympy/core/basic.py"}),
                json.dumps({"event": "file_extract_started", "file": "sympy/core/_print_helpers.py"}),
            ]
        )

        progress = _index_progress_summary(stderr)

        self.assertTrue(progress["profile_available"])
        self.assertEqual(progress["files_started"], 2)
        self.assertEqual(progress["files_completed"], 1)
        self.assertEqual(progress["last_file"], "sympy/core/_print_helpers.py")

    def test_candidate_context_requires_gold_file_hit(self) -> None:
        task = {"gold_files": ["sympy/core/_print_helpers.py"]}

        self.assertTrue(_candidate_context_has_gold_hit(task, ["sympy/core/_print_helpers.py"], ""))
        self.assertTrue(_candidate_context_has_gold_hit(task, [], "candidate: sympy/core/_print_helpers.py"))
        self.assertFalse(_candidate_context_has_gold_hit(task, ["sympy/core/symbol.py"], "Printable"))

    def test_candidate_spool_context_never_satisfies_codegraph_attribution(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            runner = self._runner(root / "out", task_fixture=None)
            repo = root / "repo"
            repo.mkdir()
            spool = root / "spool.jsonl"
            spool.write_text("sympy/core/_print_helpers.py\n", encoding="utf-8")

            def fake_run(command, command_id, *, cwd=None, timeout_s=120):  # type: ignore[no-untyped-def]
                return {
                    "argv": command,
                    "command_id": command_id,
                    "stdout": json.dumps({"files": ["sympy/core/_print_helpers.py"]}),
                    "stderr": "",
                    "exit_code": 0,
                    "success": True,
                    "failure_kind": "none",
                    "wall_time_ms": 1,
                }

            runner._run_command = fake_run  # type: ignore[method-assign]
            context = runner._staged_candidate_context(
                {"task": "Symbol __dict__ __slots__", "gold_files": ["sympy/core/_print_helpers.py"], "gold_symbols": []},
                repo,
                "codegraph_exact_text",
                {"status": "blocked_index_timeout", "db_path": str(root / "missing.sqlite"), "candidate_spool_path": str(spool)},
            )

        self.assertFalse(context["context_valid_for_attribution"])
        self.assertFalse(context["claimability"]["graph_proof"])
        self.assertFalse(context["claimability"]["graph_proof_available"])
        self.assertTrue(context["claimability"]["candidate_only"])

    def test_patch_file_quality_splits_source_test_and_extra_edits(self) -> None:
        patch = (
            "diff --git a/sympy/core/_print_helpers.py b/sympy/core/_print_helpers.py\n"
            "--- a/sympy/core/_print_helpers.py\n"
            "+++ b/sympy/core/_print_helpers.py\n"
            "@@ -1 +1 @@\n"
            "-old\n"
            "+new\n"
            "diff --git a/sympy/core/tests/test_symbol.py b/sympy/core/tests/test_symbol.py\n"
            "--- a/sympy/core/tests/test_symbol.py\n"
            "+++ b/sympy/core/tests/test_symbol.py\n"
            "@@ -1 +1 @@\n"
            "-old\n"
            "+new\n"
        )

        quality = _patch_file_quality(patch, ["sympy/core/_print_helpers.py"])

        self.assertEqual(quality["source_file_edits"], ["sympy/core/_print_helpers.py"])
        self.assertEqual(quality["test_file_edits"], ["sympy/core/tests/test_symbol.py"])
        self.assertEqual(quality["extra_file_edits"], ["sympy/core/tests/test_symbol.py"])
        self.assertEqual(quality["wrong_file_edits"], 1)

    def test_clean_source_patch_annotation_requires_eval_and_no_extra_edits(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            runner = self._runner(Path(tmp) / "out", task_fixture=None)
            result = {
                "patch_applied_static": True,
                "extra_file_edits": ["sympy/core/tests/test_symbol.py"],
                "test_file_edits": ["sympy/core/tests/test_symbol.py"],
                "swebench": {"status": "completed", "resolved": True},
            }

            runner._annotate_clean_patch_quality(result, {"gold_files": ["sympy/core/_print_helpers.py"]})

        self.assertIs(result["clean_source_patch"], False)
        self.assertEqual(result["clean_source_patch_reason"], "extra_file_edits")

    def test_is_test_path_recognizes_common_test_locations(self) -> None:
        self.assertTrue(_is_test_path("sympy/core/tests/test_symbol.py"))
        self.assertTrue(_is_test_path("tests/test_smoke.py"))
        self.assertFalse(_is_test_path("sympy/core/_print_helpers.py"))


if __name__ == "__main__":
    unittest.main()
