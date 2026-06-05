import unittest
from pathlib import Path

from benchmarks.harness.config import load_config, validate_config
from benchmarks.harness.runners.verify_benchmark_setup import verify_setup


class SetupVerificationTests(unittest.TestCase):
    def test_required_configs_validate(self):
        for path in [
            "benchmarks/configs/internal_gold_smoke.toml",
            "benchmarks/configs/internal_gold_full.toml",
            "benchmarks/configs/repobench_smoke.toml",
            "benchmarks/configs/repobench_small.toml",
            "benchmarks/configs/crosscodeeval_smoke.toml",
            "benchmarks/configs/crosscodeeval_small.toml",
            "benchmarks/configs/swebench_lite_smoke.toml",
            "benchmarks/configs/swebench_lite_10.toml",
            "benchmarks/configs/patch_runner_external_agent.example.toml",
            "benchmarks/configs/codex_external_agent.example.toml",
        ]:
            with self.subTest(path=path):
                self.assertEqual(validate_config(load_config(path)), [])

    def test_verify_setup_returns_precise_external_statuses(self):
        summary = verify_setup()
        self.assertIn(summary["status"], {"ready_for_dedicated_internal_run", "ready_with_external_blockers"})
        self.assertEqual(summary["adapters"]["internal_gold"]["status"], "ready")
        self.assertIn("repobench", summary["adapters"])
        self.assertIn("crosscodeeval", summary["adapters"])
        self.assertIn("swe_bench_lite", summary["adapters"])
        self.assertTrue(summary["docs"]["benchmark_claims"])

    def test_setup_verifier_and_preflight_share_canonical_external_agent_command(self):
        readiness_agent = load_config("benchmarks/configs/benchmark_v1_readiness.template.toml").raw["agent"]
        smoke_agent = load_config("benchmarks/tracks/swebench_lite/configs/smoke.toml").raw["agent"]
        self.assertEqual(readiness_agent["external_agent_command_env"], smoke_agent["external_agent_command_env"])
        self.assertEqual(readiness_agent["external_agent_command"], smoke_agent["external_agent_command"])
        self.assertEqual(readiness_agent["external_agent_dry_run_args"], smoke_agent["external_agent_dry_run_args"])

    def test_swebench_live_gold_script_preserves_bash_variables(self):
        script = Path("benchmarks/tracks/swebench_lite/scripts/run_harness_linux_container.ps1").read_text(
            encoding="utf-8"
        )
        self.assertIn("$script = @'", script)
        self.assertIn('if [ ! -d "$SWEBENCH" ]', script)
        self.assertIn('python3 -m pip install --break-system-packages -e "$SWEBENCH"', script)
        self.assertIn('-e "INSTANCE_ID=$InstanceId"', script)
        self.assertIn('--instance_ids "$INSTANCE_ID"', script)


if __name__ == "__main__":
    unittest.main()
