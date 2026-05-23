import json
import os
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmarks.harness.adapters.internal_gold_adapter import InternalGoldAdapter
from benchmarks.harness.config import load_config, validate_config
from benchmarks.harness.schema import new_result, validate_result
from benchmarks.harness.v1_readiness import (
    DEFAULT_CONFIG_PATH,
    DEFAULT_MANIFEST_PATH,
    provider_visible_v1_task,
    run_v1_preflight,
    validate_v1_config_template,
    validate_v1_result_schema,
    validate_v1_task_manifest,
)


TEST_OUTPUT_DIR = Path("reports/audit/artifacts/benchmark_v1_readiness_scaffold/test_outputs/unit")
REPORT_JSON = Path("reports/final/benchmark_v1_readiness_scaffold.json")


class BenchmarkV1ReadinessScaffoldTests(unittest.TestCase):
    def test_v1_config_template_validates(self):
        result = validate_v1_config_template(DEFAULT_CONFIG_PATH)
        self.assertEqual(result["status"], "pass", result["errors"])

    def test_v1_task_manifest_template_validates(self):
        result = validate_v1_task_manifest(DEFAULT_MANIFEST_PATH)
        self.assertEqual(result["status"], "pass", result["errors"])
        self.assertTrue(result["pinning_required"])

    def test_v1_result_schema_serializes(self):
        result = validate_v1_result_schema()
        self.assertEqual(result["status"], "pass", result["errors"])
        json.dumps(result["sample_result"])
        self.assertFalse(result["sample_result"]["public_claim"])
        self.assertFalse(result["sample_result"]["real_patch_quality_claim"])

    def test_old_benchmark_configs_tasks_results_still_load(self):
        for path in (
            "benchmarks/configs/internal_gold_smoke.toml",
            "benchmarks/configs/internal_gold_full.toml",
            "benchmarks/configs/repobench_smoke.toml",
            "benchmarks/configs/crosscodeeval_smoke.toml",
            "benchmarks/configs/swebench_lite_smoke.toml",
        ):
            with self.subTest(path=path):
                self.assertEqual(validate_config(load_config(path)), [])
        task = InternalGoldAdapter(Path("benchmarks/datasets/internal_gold")).load_tasks(limit=1)[0]
        result = new_result(run_id="v1-compat", task_id=task["task_id"], mode="rg_only")
        self.assertEqual(validate_result(result), [])

    def test_external_agent_missing_blocks_patch_quality_cleanly(self):
        with patch.dict(os.environ, {}, clear=True):
            summary = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "missing_external")
        self.assertEqual(summary["status"], "pass")
        self.assertFalse(summary["external_agent"]["configured"])
        self.assertEqual(summary["patch_quality"]["status"], "patch_quality_blocked_missing_external_agent")
        self.assertFalse(summary["patch_quality"]["runs_enabled"])

    def test_mock_agent_is_scaffold_only(self):
        summary = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "mock_agent")
        self.assertEqual(summary["mock_agent"]["status"], "scaffold_only")
        self.assertFalse(summary["mock_agent"]["counts_as_model_quality"])
        self.assertFalse(summary["mock_agent"]["counts_as_patch_quality"])
        self.assertFalse(summary["mock_agent"]["counts_as_product_value"])

    def test_public_claim_false_by_default_and_request_blocks(self):
        summary = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "public_default")
        self.assertFalse(summary["public_claim"])
        self.assertEqual(summary["public_claim_mode"]["status"], "disabled")
        blocked = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "public_requested", public_claim=True)
        self.assertEqual(blocked["status"], "blocked")
        self.assertIn("public claim mode is not allowed", " ".join(blocked["blocked_reasons"]))

    def test_same_agent_ab_invariant_preflight_works(self):
        summary = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "same_agent")
        invariant = summary["same_agent_ab_invariant"]
        self.assertTrue(invariant["pass"])
        self.assertTrue(invariant["normal_tools_preserved"])
        self.assertTrue(invariant["codegraph_only_extra_capability"])

    def test_rg_availability_is_required_for_both_arms(self):
        with patch("benchmarks.harness.v1_readiness._resolve_rg", return_value=None):
            summary = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "missing_rg")
        self.assertEqual(summary["status"], "blocked")
        self.assertEqual(summary["rg_availability"]["status"], "blocked")
        self.assertIn("rg availability check failed", summary["blocked_reasons"])

    def test_codegraph_availability_is_required_only_for_b_arm(self):
        summary = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "missing_codegraph", codegraph_available=False)
        self.assertEqual(summary["status"], "blocked")
        self.assertTrue(summary["codegraph_availability"]["required_for_b_arm_only"])
        self.assertIn("CodeGraph release binary is unavailable for B arm", summary["blocked_reasons"])

    def test_no_gold_evaluator_leakage_into_agent_visible_inputs(self):
        manifest = json.loads(DEFAULT_MANIFEST_PATH.read_text(encoding="utf-8"))
        task = manifest["tasks"][0]
        provider = provider_visible_v1_task(task)
        provider_text = json.dumps(provider, sort_keys=True)
        for forbidden in (
            "hidden_gold_files",
            "hidden_gold_symbols",
            "forbidden_files",
            "forbidden_symbols",
            "oracle_labels",
            "evaluator_notes",
        ):
            self.assertNotIn(forbidden, provider)
            self.assertNotIn(forbidden, provider_text)
        self.assertTrue(provider["leakage_audit"]["pass"])

    def test_generated_report_json_contract_if_present(self):
        if not REPORT_JSON.exists():
            self.skipTest("final report JSON is created by the readiness scaffold prompt")
        data = json.loads(REPORT_JSON.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "complete")
        self.assertTrue(data["benchmark_v1_scaffold_ready"])
        self.assertFalse(data["patch_quality_runs_enabled"])
        self.assertFalse(data["public_claim"])
        self.assertFalse(data["real_patch_quality_claim"])


if __name__ == "__main__":
    unittest.main()
