import unittest

from benchmarks.harness.scoring.claimability import claimability_violations, no_proof_behavior_ok
from benchmarks.harness.scoring.evidence_alignment import (
    changed_files_missing_from_context,
    evidence_alignment_summary,
    rg_flood_count,
)
from benchmarks.harness.scoring.hallucination import unsupported_claim_violations


class V0ScoringDiagnosticsTests(unittest.TestCase):
    def test_claimability_and_unsupported_overclaims_are_caught(self):
        packet = {
            "claimability": {"graph_proof": True, "proof_strength": "text_evidence"},
            "text_evidence": [{"proof_strength": "text_evidence", "graph_proof": True}],
        }
        self.assertGreater(len(claimability_violations(packet)), 0)
        self.assertGreater(unsupported_claim_violations(packet), 0)

    def test_no_proof_expected_accepts_non_graph_context(self):
        task = {"expected_claimability": {"expected_proof_status": "no_proof_path_found"}}
        packet = {"claimability": {"graph_proof": False}}
        self.assertTrue(no_proof_behavior_ok(task, packet))

    def test_rg_flood_count(self):
        self.assertEqual(rg_flood_count({"risks": [{"kind": "rg_flood"}, {"kind": "other"}]}), 1)

    def test_evidence_alignment_finds_missing_context_and_wrong_file(self):
        patch = """diff --git a/src/gold.py b/src/gold.py
--- a/src/gold.py
+++ b/src/gold.py
+KnownSymbol()
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
+ImaginarySymbol()
"""
        packet = {"files": ["src/gold.py"], "risks": [{"kind": "rg_flood"}]}
        summary = evidence_alignment_summary(
            patch_text=patch,
            packet=packet,
            gold_files=["src/gold.py"],
            known_symbols={"KnownSymbol"},
        )
        self.assertEqual(summary["wrong_file_edits"], 1)
        self.assertEqual(summary["changed_files_missing_from_context"], ["README.md"])
        self.assertEqual(summary["rg_flood_count"], 1)
        self.assertGreater(summary["nonexistent_symbol_refs"], 0)
        self.assertEqual(changed_files_missing_from_context(patch, packet), ["README.md"])


if __name__ == "__main__":
    unittest.main()
