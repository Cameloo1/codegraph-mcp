import unittest
from pathlib import Path

from benchmarks.harness.context_providers.base import ProviderBudget
from benchmarks.harness.context_providers.codegraph_exact_text import CodeGraphExactTextProvider
from benchmarks.harness.context_providers.codegraph_full import CodeGraphFullProvider
from benchmarks.harness.context_providers.none import NoneProvider
from benchmarks.harness.context_providers.rg_only import RgOnlyProvider


class ContextProviderSmokeTests(unittest.TestCase):
    def test_none_provider_returns_unknown(self):
        packet = NoneProvider(Path.cwd(), Path("benchmarks/workspaces/test")).get_context(
            {"task_id": "t", "task": "anything"}, ProviderBudget()
        )
        self.assertIn("no context provider enabled", packet.unknowns)

    def test_rg_provider_does_not_claim_graph_proof(self):
        packet = RgOnlyProvider(Path.cwd(), Path("benchmarks/workspaces/test")).get_context(
            {
                "task_id": "t",
                "task": "find README",
                "repo_path": "fixtures/buildroot_text_evidence_mini",
                "query_terms": ["generic-package"],
            },
            ProviderBudget(max_tool_calls=1, max_context_bytes=5000),
        )
        self.assertFalse(packet.claimability["graph_proof"])

    def test_provider_isolation_metadata(self):
        repo = Path.cwd()
        workspace = Path("benchmarks/workspaces/test")
        release = Path("target/release/codegraph-mcp.exe")
        none = NoneProvider(repo, workspace, release).metadata()
        rg = RgOnlyProvider(repo, workspace, release).metadata()
        exact = CodeGraphExactTextProvider(repo, workspace, release).metadata()
        full = CodeGraphFullProvider(repo, workspace, release).metadata()
        self.assertFalse(none["uses_rg"])
        self.assertFalse(none["uses_codegraph"])
        self.assertTrue(rg["uses_rg"])
        self.assertFalse(rg["uses_codegraph"])
        self.assertTrue(exact["uses_codegraph"])
        self.assertFalse(exact["uses_rg"])
        self.assertFalse(exact["vector_candidates_enabled"])
        self.assertTrue(full["uses_codegraph"])
        self.assertFalse(full["uses_rg"])
        self.assertTrue(full["vector_candidates_enabled"])


if __name__ == "__main__":
    unittest.main()
