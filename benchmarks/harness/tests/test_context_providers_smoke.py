import unittest
from pathlib import Path
from unittest.mock import patch

from benchmarks.harness.context_providers.base import ContextPacket, ProviderBudget
from benchmarks.harness.context_providers.codegraph_exact_text import (
    CodeGraphExactTextProvider,
    _merge_codegraph_json,
    _query_routes_for_term,
    _rerank_for_task_profile,
)
from benchmarks.harness.context_providers.codegraph_full import CodeGraphFullProvider
from benchmarks.harness.context_providers.codegraph_planned import CodeGraphPlannedProvider
from benchmarks.harness.context_providers.none import NoneProvider
from benchmarks.harness.context_providers.rg_only import RgOnlyProvider
from benchmarks.harness.logging_utils import CommandRecord
from benchmarks.harness.context_providers.rg_planned import RgPlannedProvider


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
        rg_planned = RgPlannedProvider(repo, workspace, release).metadata()
        exact = CodeGraphExactTextProvider(repo, workspace, release).metadata()
        full = CodeGraphFullProvider(repo, workspace, release).metadata()
        codegraph_planned = CodeGraphPlannedProvider(repo, workspace, release).metadata()
        self.assertFalse(none["uses_rg"])
        self.assertFalse(none["uses_codegraph"])
        self.assertTrue(rg["uses_rg"])
        self.assertFalse(rg["uses_codegraph"])
        self.assertTrue(rg_planned["uses_rg"])
        self.assertFalse(rg_planned["uses_codegraph"])
        self.assertFalse(rg_planned["graph_proof"])
        self.assertTrue(exact["uses_codegraph"])
        self.assertFalse(exact["uses_rg"])
        self.assertFalse(exact["vector_candidates_enabled"])
        self.assertTrue(full["uses_codegraph"])
        self.assertFalse(full["uses_rg"])
        self.assertTrue(full["vector_candidates_enabled"])
        self.assertTrue(codegraph_planned["uses_codegraph"])
        self.assertFalse(codegraph_planned["uses_rg"])
        self.assertTrue(codegraph_planned["uses_retrieval_plan"])

    def test_codegraph_routes_symbol_terms_to_symbol_query_first(self):
        self.assertEqual(_query_routes_for_term("DbPreflightReport")[0], "symbols")
        self.assertEqual(_query_routes_for_term("open_store_with_preflight")[0], "symbols")
        self.assertIn("text", _query_routes_for_term("read-only"))
        self.assertIn("files", _query_routes_for_term("fixtures/language_coverage_matrix"))

    def test_codegraph_merge_keeps_candidate_files_candidate_only(self):
        packet = ContextPacket(claimability={"graph_proof": False})
        _merge_codegraph_json(
            packet,
            {
                "candidates": [
                    {
                        "path": "expected_text_evidence.json",
                        "graph_proof": False,
                        "proof_status": "not_graph_proof",
                    }
                ],
                "routing_packet": {
                    "text_evidence": [
                        {
                            "file": "package/foo/Config.in",
                            "graph_proof": False,
                            "proof_status": "not_graph_proof",
                        }
                    ]
                },
                "graph_proof": False,
                "proof_status": "not_graph_proof",
                "claimable": True,
            },
        )
        self.assertEqual(packet.files[:2], ["expected_text_evidence.json", "package/foo/Config.in"])
        self.assertFalse(packet.claimability["graph_proof"])
        self.assertEqual(packet.claimability["proof_status"], "not_graph_proof")

    def test_codegraph_self_debug_rerank_prefers_source_over_docs(self):
        packet = ContextPacket(
            files=[
                "docs/troubleshooting.md",
                "README.md",
                "crates/codegraph-cli/src/audit.rs",
                "crates/codegraph-store/src/sqlite.rs",
            ],
            claimability={"graph_proof": False},
        )
        _rerank_for_task_profile(
            packet,
            {
                "repo_kind": "codegraph_self",
                "task_type": "db_lifecycle_debug",
            },
        )
        self.assertEqual(
            packet.files[:2],
            ["crates/codegraph-cli/src/audit.rs", "crates/codegraph-store/src/sqlite.rs"],
        )
        self.assertFalse(packet.claimability["graph_proof"])

    def test_codegraph_targeted_queries_precede_context_and_use_absolute_db(self):
        calls: list[list[str]] = []

        def fake_run(command, cwd, log_path=None, timeout_s=None):
            calls.append(command)
            if "index" in command:
                return CommandRecord(command, str(cwd), 0, "{}", "", 1, log_path)
            if "query" in command and "symbols" in command:
                stdout = (
                    '{"results":[{"file":"crates/codegraph-store/src/lib.rs",'
                    '"symbol":"DbPreflightReport"}],"graph_proof":false,"claimable":true}'
                )
                return CommandRecord(command, str(cwd), 0, stdout, "", 1, log_path)
            if "context-pack" in command:
                stdout = '{"routing_packet":{"critical_files":[{"file":"templates/skills/trace-dataflow/SKILL.md"}]},"graph_proof":false}'
                return CommandRecord(command, str(cwd), 0, stdout, "", 1, log_path)
            return CommandRecord(command, str(cwd), 0, '{"results":[],"graph_proof":false}', "", 1, log_path)

        provider = CodeGraphExactTextProvider(Path.cwd(), Path("benchmarks/workspaces/test"), Path(__file__))
        task = {
            "task_id": "self_db_lifecycle_preflight",
            "task": "Trace the CodeGraph DB lifecycle preflight implementation and status surfaces.",
            "repo_path": ".",
            "query_terms": ["DbPreflightReport"],
        }
        with patch("benchmarks.harness.context_providers.codegraph_exact_text.run_command", side_effect=fake_run):
            packet = provider.get_context(task, ProviderBudget(max_tool_calls=4, max_context_bytes=90000))

        query_index = next(index for index, command in enumerate(calls) if "query" in command)
        context_index = next(index for index, command in enumerate(calls) if "context-pack" in command)
        db_arg = calls[0][calls[0].index("--db") + 1]
        self.assertLess(query_index, context_index)
        self.assertTrue(Path(db_arg).is_absolute())
        self.assertEqual(packet.files[0], "crates/codegraph-store/src/lib.rs")
        self.assertFalse(packet.claimability["graph_proof"])

    def test_codegraph_full_vector_failure_does_not_block_context(self):
        calls: list[list[str]] = []

        def fake_run(command, cwd, log_path=None, timeout_s=None):
            calls.append(command)
            if "index" in command and "--build-vector-index" in command:
                return CommandRecord(command, str(cwd), -1, "", "TIMEOUT after 60s", 60000, log_path)
            if "index" in command:
                return CommandRecord(command, str(cwd), 0, "{}", "", 1, log_path)
            if "context-pack" in command:
                stdout = '{"routing_packet":{"critical_files":[{"file":"crates/codegraph-cli/src/lib.rs"}]},"graph_proof":false}'
                return CommandRecord(command, str(cwd), 0, stdout, "", 1, log_path)
            return CommandRecord(command, str(cwd), 0, '{"results":[],"graph_proof":false}', "", 1, log_path)

        provider = CodeGraphFullProvider(Path.cwd(), Path("benchmarks/workspaces/test"), Path(__file__))
        task = {
            "task_id": "self_vector_chunk_accounting",
            "task": "Trace vector chunk index build accounting.",
            "repo_path": ".",
            "query_terms": ["vector chunk"],
        }
        with patch("benchmarks.harness.context_providers.codegraph_exact_text.run_command", side_effect=fake_run):
            packet = provider.get_context(task, ProviderBudget(max_tool_calls=3, max_context_bytes=90000))

        self.assertIn("crates/codegraph-cli/src/lib.rs", packet.files)
        self.assertTrue(any(risk.get("kind") == "vector_sidecar_unavailable" for risk in packet.risks))
        self.assertTrue(any("context-pack" in command for command in calls))
        self.assertFalse(packet.claimability["graph_proof"])


if __name__ == "__main__":
    unittest.main()
