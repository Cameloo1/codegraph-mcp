import unittest

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


if __name__ == "__main__":
    unittest.main()
