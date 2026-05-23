from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

from benchmarks.harness.resource_guard import ResourceLimits
from benchmarks.harness.runners.run_swebench_patch_smoke import (
    BASE_COMMIT,
    BenchmarkBlocked,
    PatchSmokeRunner,
    TASK_ID,
    _candidate_context_has_gold_hit,
    _candidate_query_index_path,
    _index_progress_summary,
    _is_test_path,
    _patch_file_quality,
    _prebuild_status,
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

    def test_candidate_query_index_path_matches_release_sidecar_name(self) -> None:
        spool = Path("sympy__sympy-20590.candidate_spool.jsonl")

        self.assertEqual(_candidate_query_index_path(spool).name, "sympy__sympy-20590.candidate_spool.query.sqlite")

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
