import json
import os
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmarks.harness.adapters.internal_gold_adapter import InternalGoldAdapter
from benchmarks.harness.agent_reliability_registry import (
    load_agent_reliability_tasks,
    provider_visible_agent_task,
)
from benchmarks.harness.config import load_config, validate_config
from benchmarks.harness.runners.run_ab_agent_harness import (
    CODEGRAPH_TOOL,
    DEFAULT_AGENT_SCAFFOLD,
    DEFAULT_BUDGET,
    DEFAULT_EVALUATOR,
    DEFAULT_MODEL,
    NORMAL_TOOLS,
    PATCH_MODES,
    build_agent_visible_prompt,
    evaluator_only_field_findings,
    run_ab_harness,
    run_mode,
)
from benchmarks.harness.schema import (
    AGENT_RELIABILITY_EVALUATOR_ONLY_FIELDS,
    new_result,
    validate_result,
)


REPORT_JSON = Path("reports/audit/ab_agent_harness_scaffold.json")
TEST_OUTPUT_DIR = Path("reports/audit/artifacts/ab_agent_harness_scaffold/test_outputs/unit")


class ABAgentHarnessScaffoldTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tasks = load_agent_reliability_tasks()
        cls.by_id = {task["task_id"]: task for task in cls.tasks}

    def _run(self, name: str, **kwargs):
        output_dir = TEST_OUTPUT_DIR / name
        return run_ab_harness(output_dir=output_dir, max_tasks=1, **kwargs)

    def test_same_task_budget_model_scaffold_metadata_across_ab(self):
        summary = self._run("same_metadata")
        plan = json.loads(Path(summary["run_plan"]).read_text(encoding="utf-8"))
        arm_a = plan["arms"]["A"]
        arm_b = plan["arms"]["B"]
        for field in ("model", "agent_scaffold", "evaluator", "budget"):
            self.assertEqual(arm_a[field], arm_b[field], field)
        self.assertEqual(arm_a["budget"], DEFAULT_BUDGET)
        self.assertTrue(summary["same_agent_invariant"]["pass"])
        self.assertEqual(len(plan["tasks"]), 1)

    def test_same_visible_task_input_across_ab(self):
        summary = self._run("same_visible_input")
        hashes = {
            result["mode"]: result["sanitized_task_sha"]
            for result in summary["results"]
            if result["mode"] in {"rg_only_agent_plan", "rg_plus_codegraph_agent_plan"}
        }
        self.assertEqual(hashes["rg_only_agent_plan"], hashes["rg_plus_codegraph_agent_plan"])

    def test_a_arm_still_has_normal_rg_search_edit_test_tools(self):
        summary = self._run("a_tools")
        plan = json.loads(Path(summary["run_plan"]).read_text(encoding="utf-8"))
        self.assertEqual(plan["arms"]["A"]["tools"], list(NORMAL_TOOLS))
        self.assertFalse(plan["arms"]["A"]["uses_codegraph"])

    def test_b_arm_has_normal_tools_plus_codegraph_only(self):
        summary = self._run("b_tools")
        plan = json.loads(Path(summary["run_plan"]).read_text(encoding="utf-8"))
        self.assertEqual(plan["arms"]["B"]["tools"], list(NORMAL_TOOLS) + [CODEGRAPH_TOOL])
        self.assertEqual(set(plan["arms"]["B"]["tools"]) - set(plan["arms"]["A"]["tools"]), {CODEGRAPH_TOOL})

    def test_codegraph_unavailable_blocks_b_arm_without_silent_advantage(self):
        summary = self._run(
            "codegraph_unavailable",
            modes=["rg_plus_codegraph_agent_plan"],
            codegraph_available=False,
        )
        result = summary["results"][0]
        self.assertEqual(result["status"], "blocked")
        self.assertFalse(result["quality_claim"])
        self.assertIn("CodeGraph context is unavailable", result["blocked_reason"])
        self.assertEqual(summary["codegraph_availability"]["status"], "blocked")

    def test_external_agent_missing_blocks_patch_quality_modes(self):
        with patch.dict(os.environ, {}, clear=True):
            summary = self._run(
                "patch_missing_external",
                modes=sorted(PATCH_MODES),
                enable_patch_modes=True,
                external_agent_command="",
            )
        self.assertEqual({result["status"] for result in summary["results"]}, {"blocked"})
        for result in summary["results"]:
            self.assertFalse(result["patch_mode_enabled"])
            self.assertIn("external agent command is not configured", result["blocked_reason"])
            self.assertFalse(result["real_patch_quality_claim"])

    def test_deterministic_plan_only_mode_runs_on_fixture(self):
        summary = self._run("deterministic_plan_only", modes=["deterministic_plan_only"])
        self.assertEqual(summary["status"], "complete")
        result = summary["results"][0]
        self.assertEqual(result["status"], "diagnostic_only")
        self.assertIn("agent_plan", result)
        self.assertIn("scores", result)
        plan_matches = list((Path(summary["output_dir"]) / "per_task").glob("*/deterministic_plan_only/agent_plan.json"))
        self.assertEqual(len(plan_matches), 1)
        plan_json = plan_matches[0]
        plan_md = plan_json.with_suffix(".md")
        self.assertTrue(plan_json.exists())
        self.assertTrue(plan_md.exists())

    def test_mock_agent_mode_is_scaffold_only_and_not_product_quality(self):
        summary = self._run("mock_agent", modes=["mock_agent_scaffold_only"])
        result = summary["results"][0]
        self.assertEqual(result["status"], "scaffold_only")
        self.assertEqual(result["mock_agent_status"], "mock_non_quality")
        self.assertFalse(result["quality_claim"])
        self.assertFalse(result["real_patch_quality_claim"])

    def test_scorer_outputs_parse(self):
        summary = self._run("scorer_outputs", modes=["rg_only_agent_plan"])
        scores = summary["results"][0]["scores"]
        self.assertIn("plan_accuracy", scores)
        self.assertIn("proof_discipline", scores)
        self.assertIn("hallucination_trap", scores)
        self.assertIsInstance(summary["results"][0]["scorer_failure_reasons"], list)
        json.dumps(scores)

    def test_no_gold_leakage_into_provider_visible_fields_or_agent_prompt(self):
        task = self.by_id["ar_investigation_benchmark_query_leakage"]
        provider_task = provider_visible_agent_task(task)
        prompt = build_agent_visible_prompt(provider_task, "rg_plus_codegraph_agent_plan")
        prompt_obj = json.loads(prompt)
        self.assertEqual(evaluator_only_field_findings(prompt_obj), [])
        for field in AGENT_RELIABILITY_EVALUATOR_ONLY_FIELDS:
            self.assertNotIn(field, provider_task)
            self.assertNotIn(field, prompt)
        self.assertNotIn("HiddenGoldTargetSymbol", prompt)
        self.assertNotIn("secret/evaluator/gold_target.py", prompt)

        result = run_mode(
            task=task,
            sanitized_task=provider_task,
            mode="rg_plus_codegraph_agent_plan",
            output_dir=TEST_OUTPUT_DIR / "leakage" / "rg_plus_codegraph_agent_plan",
            model=DEFAULT_MODEL,
            agent_scaffold=DEFAULT_AGENT_SCAFFOLD,
            evaluator=DEFAULT_EVALUATOR,
            budget=DEFAULT_BUDGET,
            enable_patch_modes=False,
            external_agent_command="",
            codegraph_available=True,
        )
        self.assertTrue(result["leakage_audit_pass"])
        self.assertTrue(result["agent_visible_leakage_pass"])

    def test_patch_modes_disabled_by_default(self):
        summary = self._run("patch_disabled_default", modes=sorted(PATCH_MODES))
        self.assertFalse(summary["patch_modes_enabled_by_default"])
        self.assertEqual({result["status"] for result in summary["results"]}, {"not_configured"})
        for result in summary["results"]:
            self.assertFalse(result["patch_mode_enabled"])
            self.assertEqual(result["patch_outcome_placeholder"]["status"], "disabled")

    def test_generated_report_json_contract(self):
        if not REPORT_JSON.exists():
            self.skipTest("audit report JSON is created after implementation")
        data = json.loads(REPORT_JSON.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "complete")
        self.assertFalse(data["public_claim"])
        self.assertFalse(data["real_patch_quality_claim"])
        self.assertFalse(data["normal_dot_codegraph_mutated"])
        self.assertTrue(data["ab_harness_scaffold_created"])
        self.assertTrue(data["plan_only_diagnostic_ready"])
        self.assertFalse(data["patch_modes_enabled_by_default"])
        self.assertTrue(data["same_agent_invariant_enforced"])
        self.assertTrue(data["gold_leakage_prevented"])

    def test_old_benchmark_configs_tasks_results_still_load(self):
        config_paths = [
            "benchmarks/configs/internal_gold_smoke.toml",
            "benchmarks/configs/internal_gold_full.toml",
            "benchmarks/configs/repobench_smoke.toml",
            "benchmarks/configs/crosscodeeval_smoke.toml",
            "benchmarks/configs/swebench_lite_smoke.toml",
        ]
        for path in config_paths:
            with self.subTest(path=path):
                self.assertEqual(validate_config(load_config(path)), [])

        old_tasks = InternalGoldAdapter(Path("benchmarks/datasets/internal_gold")).load_tasks(limit=1)
        self.assertEqual(len(old_tasks), 1)
        result = new_result(run_id="ab-compat", task_id=old_tasks[0]["task_id"], mode="rg_only")
        self.assertEqual(validate_result(result), [])
        json.dumps(result)


if __name__ == "__main__":
    unittest.main()
