import unittest

from benchmarks.harness.schema import new_result, validate_result


class MetricsSchemaTests(unittest.TestCase):
    def test_valid_result_passes(self):
        result = new_result(run_id="r1", task_id="t1", mode="rg_only")
        self.assertEqual(validate_result(result), [])

    def test_invalid_claimability_fails(self):
        result = new_result(run_id="r1", task_id="t1", trust={"claimability_violations": 1})
        self.assertIn("result contains claimability violations", validate_result(result))


if __name__ == "__main__":
    unittest.main()

