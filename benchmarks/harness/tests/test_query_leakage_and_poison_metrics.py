import tempfile
import unittest
from pathlib import Path

from benchmarks.harness.adapters.crosscodeeval_adapter import CrossCodeEvalAdapter
from benchmarks.harness.adapters.repobench_adapter import RepoBenchAdapter
from benchmarks.harness.scoring.context_recall import score_retrieval
from benchmarks.harness.task_sanitizer import audit_provider_task, sanitize_provider_task


class QueryLeakageAndPoisonMetricTests(unittest.TestCase):
    def test_repobench_adapter_does_not_expose_hidden_gold_filename(self):
        row = {
            "task_id": "leak_repobench",
            "prompt": "Normalize the incoming user value before saving it.",
            "crossfile_context": {
                "list": [
                    {
                        "filename": "registry.py",
                        "identifier": "hidden_registry_symbol",
                        "retrieved_chunk": "def hidden_registry_symbol(value): return value",
                    }
                ]
            },
        }
        task = RepoBenchAdapter("unused")._map_row(row, 0)
        provider_task = sanitize_provider_task(task)
        terms = set(provider_task["visible_query_terms"])
        self.assertNotIn("registry.py", terms)
        self.assertNotIn("hidden_registry_symbol", terms)
        self.assertTrue(provider_task["leakage_audit"]["pass"])

    def test_crosscodeeval_adapter_does_not_expose_hidden_context_filename(self):
        row = {
            "prompt": "Complete the route handler after validating the request.",
            "groundtruth": "registry.apply(handler)",
            "metadata": {"task_id": "leak_cross", "file": "server.py"},
            "crossfile_context": {
                "list": [
                    {
                        "filename": "context.py",
                        "retrieved_chunk": "def apply(handler): return handler",
                    }
                ]
            },
        }
        task = CrossCodeEvalAdapter("unused")._map_row(row, 0)
        provider_task = sanitize_provider_task(task)
        self.assertNotIn("context.py", set(provider_task["visible_query_terms"]))
        self.assertNotIn("registry.apply", set(provider_task["visible_query_terms"]))
        self.assertTrue(provider_task["leakage_audit"]["pass"])

    def test_provider_input_audit_fails_when_forbidden_keys_reach_provider(self):
        audit = audit_provider_task({"task_id": "t", "task": "visible", "gold_files": ["secret.py"]})
        self.assertFalse(audit["pass"])
        self.assertIn("gold_files", audit["forbidden_keys_present"])

    def test_query_term_hidden_gold_is_stripped_unless_visible_seed(self):
        task = {
            "task_id": "t",
            "task": "Find the implementation for the visible request handler.",
            "query_terms": ["registry.py"],
            "gold_files": ["registry.py"],
            "gold_symbols": [],
            "gold_spans": [],
        }
        provider_task = sanitize_provider_task(task)
        self.assertNotIn("registry.py", provider_task["visible_query_terms"])
        self.assertTrue(provider_task["leakage_audit"]["pass"])

        visible_seed = {**task, "visible_file_hints": ["registry.py"]}
        provider_task = sanitize_provider_task(visible_seed)
        self.assertIn("registry.py", provider_task["visible_query_terms"])
        self.assertTrue(provider_task["leakage_audit"]["pass"])

    def test_score_only_gold_file_has_high_precision_and_no_poison(self):
        metrics = score_retrieval(
            {"gold_files": ["gold.py"], "gold_symbols": [], "gold_spans": [], "forbidden_files": [], "forbidden_symbols": []},
            {"files": ["gold.py"], "symbols": [], "snippets": [], "raw_context_bytes": 100},
        )
        self.assertEqual(metrics["gold_file_recall_at_5"], 1.0)
        self.assertEqual(metrics["precision_at_1"], 1.0)
        self.assertIsNone(metrics["context_poison_count"])
        self.assertIsNone(metrics["dangerous_context_present"])

    def test_score_gold_plus_forbidden_files_penalizes_poison(self):
        metrics = score_retrieval(
            {
                "gold_files": ["gold.py"],
                "gold_symbols": [],
                "gold_spans": [],
                "forbidden_files": ["bad1.py", "bad2.py", "bad3.py", "bad4.py"],
                "forbidden_symbols": [],
            },
            {
                "files": ["gold.py", "bad1.py", "bad2.py", "bad3.py", "bad4.py"],
                "symbols": [],
                "snippets": [{"file": "bad1.py", "text": "danger"}],
                "raw_context_bytes": 500,
            },
        )
        self.assertEqual(metrics["gold_file_recall_at_5"], 1.0)
        self.assertEqual(metrics["precision_at_5"], 0.2)
        self.assertGreater(metrics["forbidden_file_hits_at_5"], 0)
        self.assertGreater(metrics["context_poison_count"], 0)
        self.assertTrue(metrics["dangerous_context_present"])

    def test_score_many_non_gold_distractors_lowers_density(self):
        metrics = score_retrieval(
            {"gold_files": ["gold.py"], "gold_symbols": [], "gold_spans": [], "forbidden_files": [], "forbidden_symbols": []},
            {"files": ["gold.py", "a.py", "b.py", "c.py", "d.py"], "symbols": [], "snippets": [], "raw_context_bytes": 1000},
        )
        self.assertEqual(metrics["wrong_context_rate_at_5"], 0.8)
        self.assertEqual(metrics["gold_density_in_context"], 0.2)

    def test_score_forbidden_symbol_without_forbidden_file(self):
        metrics = score_retrieval(
            {"gold_files": ["gold.py"], "gold_symbols": [], "gold_spans": [], "forbidden_files": [], "forbidden_symbols": ["BadSymbol"]},
            {"files": ["gold.py"], "symbols": ["BadSymbol"], "snippets": [], "raw_context_bytes": 100},
        )
        self.assertEqual(metrics["forbidden_symbol_hits_at_5"], 1)
        self.assertTrue(metrics["dangerous_context_present"])

    def test_no_forbidden_schema_marks_poison_metrics_not_applicable(self):
        metrics = score_retrieval(
            {"gold_files": ["gold.py"], "gold_symbols": [], "gold_spans": []},
            {"files": ["gold.py"], "symbols": [], "snippets": [], "raw_context_bytes": 100},
        )
        self.assertIsNone(metrics["forbidden_file_hits_at_5"])
        self.assertIsNone(metrics["context_poison_count"])


if __name__ == "__main__":
    unittest.main()
