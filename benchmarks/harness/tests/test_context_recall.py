import unittest

from benchmarks.harness.scoring.context_recall import mrr, recall_at_k, score_retrieval


class ContextRecallTests(unittest.TestCase):
    def test_recall_at_k(self):
        self.assertEqual(recall_at_k(["a.py", "b.py"], ["a.py"], 1), 0.5)
        self.assertEqual(recall_at_k(["a.py"], ["x.py", "a.py"], 1), 0.0)
        self.assertEqual(recall_at_k(["a.py"], ["x.py", "a.py"], 5), 1.0)

    def test_mrr(self):
        self.assertEqual(mrr(["target.py"], ["a.py", "target.py"]), 0.5)

    def test_score_retrieval(self):
        task = {"gold_files": ["src/lib.rs"], "gold_symbols": [], "gold_spans": []}
        packet = {"files": [{"file": "src/lib.rs"}], "raw_context_bytes": 40}
        score = score_retrieval(task, packet)
        self.assertEqual(score["gold_file_recall_at_1"], 1.0)
        self.assertEqual(score["context_tokens_estimated"], 10)


if __name__ == "__main__":
    unittest.main()

