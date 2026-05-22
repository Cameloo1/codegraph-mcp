import unittest
from pathlib import Path


class NoPublicClaimsTests(unittest.TestCase):
    def test_claims_doc_contains_unsafe_swebench_wording(self):
        text = Path("benchmarks/BENCHMARK_CLAIMS.md").read_text(encoding="utf-8")
        self.assertIn("Unsafe Wording", text)
        self.assertIn("CodeGraph gets X% on SWE-bench", text)
        self.assertIn("local diagnostic ablation", text)


if __name__ == "__main__":
    unittest.main()

