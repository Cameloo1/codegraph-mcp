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
    load_raw_toml,
    provider_visible_v1_task,
    run_v1_preflight,
    validate_v1_config_template,
    validate_v1_result_schema,
    validate_v1_task_manifest,
)


TEST_OUTPUT_DIR = Path("reports/audit/artifacts/benchmark_v1_readiness_scaffold/test_outputs/unit")
REPORT_JSON = Path("reports/final/benchmark_v1_readiness_scaffold.json")
PINNED_LADDER_MANIFESTS = {
    "one_task": (
        Path("benchmarks/datasets/v1_real_agent_patch_outcomes/swebench_lite_ladder_1_manifest.json"),
        1,
    ),
    "five_task": (
        Path("benchmarks/datasets/v1_real_agent_patch_outcomes/swebench_lite_ladder_5_manifest.json"),
        5,
    ),
    "ten_task": (
        Path("benchmarks/datasets/v1_real_agent_patch_outcomes/swebench_lite_ladder_10_manifest.json"),
        10,
    ),
}


class BenchmarkV1ReadinessScaffoldTests(unittest.TestCase):
    def test_v1_config_template_validates(self):
        result = validate_v1_config_template(DEFAULT_CONFIG_PATH)
        self.assertEqual(result["status"], "pass", result["errors"])

    def test_v1_task_manifest_template_validates(self):
        result = validate_v1_task_manifest(DEFAULT_MANIFEST_PATH)
        self.assertEqual(result["status"], "pass", result["errors"])
        self.assertTrue(result["pinning_required"])

    def test_pinned_ladder_manifests_validate_with_concrete_task_ids(self):
        for rung, (path, expected_count) in PINNED_LADDER_MANIFESTS.items():
            with self.subTest(rung=rung):
                result = validate_v1_task_manifest(path)
                self.assertEqual(result["status"], "pass", result["errors"])
                self.assertFalse(result["pinning_required"])
                data = json.loads(path.read_text(encoding="utf-8"))
                task_ids = [task["task_id"] for task in data["tasks"]]
                self.assertEqual(data["manifest_status"], "pinned")
                self.assertEqual(len(task_ids), expected_count)
                self.assertEqual(len(task_ids), len(set(task_ids)))
                self.assertFalse(any(task_id.endswith("_blocked") for task_id in task_ids))
                self.assertNotIn("PIN_REQUIRED_BEFORE_RUN", json.dumps(data))

    def test_pinned_ladder_provider_visible_fields_exclude_gold_oracle_data(self):
        forbidden_provider_keys = {
            "hidden_gold_files",
            "hidden_gold_symbols",
            "forbidden_files",
            "forbidden_symbols",
            "oracle_labels",
            "expected_tests",
            "evaluator_notes",
            "patch",
            "test_patch",
            "FAIL_TO_PASS",
            "PASS_TO_PASS",
            "hints_text",
        }
        for rung, (path, _expected_count) in PINNED_LADDER_MANIFESTS.items():
            data = json.loads(path.read_text(encoding="utf-8"))
            for task in data["tasks"]:
                with self.subTest(rung=rung, task_id=task["task_id"]):
                    provider = provider_visible_v1_task(task)
                    provider_text = json.dumps(provider, sort_keys=True)
                    self.assertTrue(provider["leakage_audit"]["pass"])
                    for forbidden in forbidden_provider_keys:
                        self.assertNotIn(forbidden, provider)
                        self.assertNotIn(forbidden, provider_text)

    def test_pinned_ladder_checkouts_and_same_agent_metadata_are_declared(self):
        for rung, (path, _expected_count) in PINNED_LADDER_MANIFESTS.items():
            data = json.loads(path.read_text(encoding="utf-8"))
            for task in data["tasks"]:
                with self.subTest(rung=rung, task_id=task["task_id"]):
                    checkout = task["repo"]["checkout"]
                    checkout_path_text = checkout["local_source_checkout_path"]
                    checkout_path = Path(checkout_path_text)
                    self.assertTrue(checkout_path_text, checkout_path)
                    self.assertTrue(checkout_path_text.startswith("benchmarks/"), checkout_path)
                    self.assertTrue(checkout["base_commit_resolved"])
                    self.assertIn("safe_directory_validation", checkout)
                    self.assertIn("docker_evaluator_checkout_policy", checkout)
                    invariant = task["same_agent_ab_invariant"]
                    self.assertTrue(invariant["same_model_scaffold_task_budget_evaluator_timeout_repo_commit"])
                    self.assertIn("CodeGraph", invariant["arm_b"])

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
            summary = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "missing_external", external_agent_command="")
        self.assertEqual(summary["status"], "pass")
        self.assertFalse(summary["external_agent"]["configured"])
        self.assertEqual(summary["patch_quality"]["status"], "patch_quality_blocked_missing_external_agent")
        self.assertFalse(summary["patch_quality"]["runs_enabled"])

    def test_default_config_declares_canonical_external_agent_wrapper(self):
        raw = load_raw_toml(DEFAULT_CONFIG_PATH)
        agent = raw["agent"]
        self.assertEqual(agent["external_agent_command_env"], "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND")
        self.assertEqual(
            agent["external_agent_command"],
            "powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/tracks/swebench_lite/scripts/run_codex_external_patch_agent.ps1",
        )
        self.assertEqual(agent["external_agent_dry_run_args"], "-ValidateOnly")

    def test_configured_wrapper_is_seen_as_external_agent_configured(self):
        with patch.dict(os.environ, {}, clear=True), patch(
            "benchmarks.harness.v1_readiness.validate_external_agent_command",
            return_value={"status": "ready", "configured": True},
        ):
            summary = run_v1_preflight(output_dir=TEST_OUTPUT_DIR / "configured_external")
        self.assertEqual(summary["external_agent"]["status"], "ready")
        self.assertTrue(summary["external_agent"]["configured"])
        self.assertEqual(summary["patch_quality"]["status"], "disabled_readiness_only")

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
