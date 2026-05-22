import unittest

from benchmarks.harness.scoring.claimability import claimability_violations, no_proof_behavior_ok


class ClaimabilityTests(unittest.TestCase):
    def test_text_evidence_cannot_be_graph_proof(self):
        packet = {"text_evidence": [{"proof_strength": "text_evidence", "graph_proof": True}]}
        self.assertTrue(claimability_violations(packet))

    def test_no_proof_expected_accepts_non_graph_packet(self):
        task = {"expected_claimability": {"expected_proof_status": "no_proof_path_found"}}
        packet = {"claimability": {"graph_proof": False}}
        self.assertTrue(no_proof_behavior_ok(task, packet))


if __name__ == "__main__":
    unittest.main()

