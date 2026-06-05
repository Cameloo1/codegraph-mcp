from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from benchmarks.harness.runners.run_swebench_patch_ladder import (
    _agent_payload_gold_leakage,
    _fixture_from_manifest_task,
    _rung_summary,
)


class SwebenchPatchLadderRunnerTests(unittest.TestCase):
    def _manifest_task(self) -> dict:
        return {
            "task_id": "swebench_lite_sympy_sympy_11400",
            "repo": {
                "repo_name": "sympy/sympy",
                "pinned_commit": "task-base-commit",
                "task_source_id": "sympy__sympy-11400",
                "checkout": {
                    "local_source_checkout_path": "benchmarks/results/summaries/swebench_focused_20260521_163620/repos/sympy-source"
                },
            },
            "visible": {
                "prompt": "ccode(sinc(x)) should not call an unsupported sinc.",
            },
            "evaluator_only": {
                "hidden_gold_files": ["sympy/printing/ccode.py"],
                "hidden_gold_symbols": ["CCodePrinter"],
                "expected_tests": ["sympy/printing/tests/test_ccode.py"],
            },
        }

    def test_manifest_task_fixture_uses_provider_visible_prompt_and_pinned_commit(self) -> None:
        fixture = _fixture_from_manifest_task(self._manifest_task())

        self.assertEqual(fixture["instance_id"], "sympy__sympy-11400")
        self.assertEqual(fixture["base_commit"], "task-base-commit")
        self.assertEqual(fixture["task"], "ccode(sinc(x)) should not call an unsupported sinc.")
        self.assertEqual(fixture["gold_files"], ["sympy/printing/ccode.py"])
        self.assertNotIn("patch", fixture)
        self.assertNotIn("test_patch", fixture)

    def test_rung_summary_preserves_attribution_and_detects_no_gold_leakage(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            task_dir = root / "tasks" / "sympy__sympy-11400"
            payload_dir = task_dir / "per_mode" / "codegraph_exact_text"
            payload_dir.mkdir(parents=True)
            (task_dir / "summary.json").write_text("{}", encoding="utf-8")
            (payload_dir / "agent_payload.json").write_text(
                json.dumps({"context": {"provider_visible_queries": [{"term": "sinc"}]}}),
                encoding="utf-8",
            )
            summary = _rung_summary(
                manifest={"rung": "one_task", "tasks": [self._manifest_task()]},
                manifest_path=Path("manifest.json"),
                output_dir=root,
                modes=["rg_only", "codegraph_exact_text"],
                task_results=[
                    {
                        "task_id": "sympy__sympy-11400",
                        "task_dir": str(task_dir),
                        "summary": {
                            "status": "complete",
                            "results_by_mode": {
                                "rg_only": {
                                    "agent": {"status": "ok", "wall_time_ms": 1},
                                    "swebench": {"status": "completed", "resolved": False},
                                },
                                "codegraph_exact_text": {
                                    "context_valid_for_attribution": True,
                                    "graph_proof_available": True,
                                    "context_claimability": {"graph_proof": False},
                                    "agent": {"status": "ok", "wall_time_ms": 1},
                                    "swebench": {"status": "completed", "resolved": True},
                                },
                            },
                        },
                    }
                ],
                before_dot_codegraph=False,
                after_dot_codegraph=False,
                skip_agent=False,
                skip_eval=False,
            )

        self.assertTrue(summary["rung_gate_passed"])
        self.assertTrue(summary["codegraph_attribution_valid"])
        self.assertEqual(summary["gold_leakage_violations"], 0)
        self.assertEqual(_agent_payload_gold_leakage(root), 0)


if __name__ == "__main__":
    unittest.main()
