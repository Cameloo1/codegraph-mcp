import unittest
from pathlib import Path

from benchmarks.harness.adapters.crosscodeeval_adapter import CrossCodeEvalAdapter
from benchmarks.harness.adapters.internal_gold_adapter import InternalGoldAdapter
from benchmarks.harness.adapters.repobench_adapter import RepoBenchAdapter


class InternalGoldAdapterTests(unittest.TestCase):
    def test_loads_at_least_twenty_tasks(self):
        tasks = InternalGoldAdapter(Path("benchmarks/datasets/internal_gold")).load_tasks()
        self.assertGreaterEqual(len(tasks), 20)
        self.assertTrue(all("task_id" in task for task in tasks))

    def test_external_adapter_fixtures_are_non_official_and_loadable(self):
        repo_tasks = RepoBenchAdapter(Path("missing")).load_fixture_tasks(limit=2)
        cross_tasks = CrossCodeEvalAdapter(Path("missing")).load_fixture_tasks(limit=2)
        self.assertEqual(len(repo_tasks), 2)
        self.assertEqual(len(cross_tasks), 2)
        self.assertEqual(repo_tasks[0]["source"], "repobench_real_or_fixture")
        self.assertEqual(cross_tasks[0]["source"], "crosscodeeval_real_or_fixture")

    def test_repobench_missing_real_data_reports_precise_blocker(self):
        status = RepoBenchAdapter(Path("benchmarks/workspaces/repobench_data")).setup_status()
        self.assertIn(status.setup_state, {"fixture_only", "skipped_with_precise_blocker", "partial_smoke_ready", "ready_for_full_run"})
        if not status.ready:
            self.assertTrue(status.blockers)

    def test_repobench_huggingface_context_rows_map_gold_files_and_symbols(self):
        row = {
            "repo_name": "example/repo",
            "file_path": "target.py",
            "context": [
                {"identifier": "Registry", "path": "pkg/registry.py", "snippet": "class Registry: ..."},
                {"identifier": "BaseProcessor", "path": "pkg/base.py", "snippet": "class BaseProcessor: ..."},
            ],
        }
        task = RepoBenchAdapter(Path("missing"))._map_row(row, 0)
        self.assertEqual(task["gold_files"], ["pkg/registry.py", "pkg/base.py"])
        self.assertEqual(task["gold_symbols"], ["Registry", "BaseProcessor"])
        self.assertEqual(task["metadata"]["target_file"], "target.py")


if __name__ == "__main__":
    unittest.main()
