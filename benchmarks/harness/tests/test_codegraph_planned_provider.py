import tempfile
import unittest
from pathlib import Path

from benchmarks.harness.context_providers.base import ProviderBudget
from benchmarks.harness.context_providers.codegraph_planned import (
    CodeGraphPlannedProvider,
    _packet_claimability,
    build_planned_queries,
    normalize_codegraph_evidence_item,
)
from benchmarks.harness.schema import new_result, validate_result
from benchmarks.harness.scoring.claimability import claimability_violations
from benchmarks.harness.timing import classify_timing_bucket


def _write(root: Path, rel: str, text: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


class CodeGraphPlannedProviderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.release = Path("target/release/codegraph-mcp.exe")
        if not cls.release.exists():
            raise unittest.SkipTest("release binary required for codegraph_planned provider tests")

    def _provider(self, workspace: Path) -> CodeGraphPlannedProvider:
        return CodeGraphPlannedProvider(Path.cwd(), workspace, self.release)

    def _run(self, repo: Path, workspace: Path, task: dict, *, max_calls: int = 8):
        provider = self._provider(workspace)
        return provider.get_context(
            {"task_id": "t", "repo_path": str(repo), **task},
            ProviderBudget(max_tool_calls=max_calls, max_context_bytes=50000, max_time_s=90),
        )

    def test_planned_queries_route_visible_clue_types(self):
        queries = build_planned_queries(
            {
                "task": "Open src/router.py, inspect TargetSymbol, and read Config.in docs.",
                "visible_query_terms": ["src/router.py", "TargetSymbol", "Config.in docs"],
                "visible_symbol_hints": ["TargetSymbol"],
                "visible_file_hints": ["src/router.py"],
            },
            {},
        )
        pairs = {(query["kind"], query["term"]) for query in queries}
        self.assertIn(("files", "src/router.py"), pairs)
        self.assertIn(("symbols", "TargetSymbol"), pairs)
        self.assertTrue(any(query["kind"] == "text" and "Config.in" in query["term"] for query in queries))

    def test_provider_executes_files_symbols_and_text_queries_with_exact_argv(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            workspace = root / "workspace"
            _write(repo, "src/router.py", "class TargetSymbol:\n    pass\n")
            _write(repo, "package/foo/Config.in", "config BR2_PACKAGE_FOO\n")
            packet = self._run(
                repo,
                workspace,
                {
                    "task": "Open src/router.py, inspect TargetSymbol, and read Config.in docs.",
                    "visible_query_terms": ["src/router.py", "TargetSymbol", "Config.in"],
                    "visible_symbol_hints": ["TargetSymbol"],
                    "visible_file_hints": ["src/router.py"],
                    "visible_text_hints": ["Config.in docs"],
                },
                max_calls=8,
            )
            argvs = [command["argv"] for command in packet.raw["commands"]]
            self.assertTrue(any("context-pack" in argv for argv in argvs))
            self.assertTrue(any(_has_query(argv, "files") for argv in argvs))
            self.assertTrue(any(_has_query(argv, "symbols") for argv in argvs))
            self.assertTrue(any(_has_query(argv, "text") for argv in argvs))
            self.assertFalse(packet.raw["normal_dot_codegraph_mutated"])
            self.assertTrue(Path(packet.raw["command_log_path"]).exists())
            self.assertEqual(len(claimability_violations(packet.to_dict())), 0)

    def test_implementation_trace_preserves_source_navigation_and_artifact_requirements(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            workspace = root / "workspace"
            _write(repo, "crates/codegraph-vector/src/lib.rs", "fn estimated_raw_f32(chunk_count: usize) -> usize { chunk_count * 4 }\n")
            packet = self._run(
                repo,
                workspace,
                {
                    "task": "Trace vector chunk index build accounting and persisted vector index size math.",
                    "task_type": "implementation_trace",
                    "visible_query_terms": ["estimated_raw_f32", "vector chunk accounting"],
                },
                max_calls=7,
            )
            self.assertTrue(packet.raw["source_navigation_evidence"])
            self.assertTrue(packet.raw["artifact_db_inspection_requirements"])
            self.assertFalse(packet.claimability["graph_proof"])
            self.assertEqual(packet.claimability["proof_strength"], "source_navigation_evidence")

    def test_no_proof_fallback_and_output_schema_validate(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            workspace = root / "workspace"
            _write(repo, "docs/context-pack.md", "no_proof_path_found fallback_snippets context-pack\n")
            packet = self._run(
                repo,
                workspace,
                {
                    "task": "Find implementation surfaces for context-pack no-proof fallback packets.",
                    "task_type": "no-proof_fallback",
                    "visible_query_terms": ["no_proof_path_found", "fallback_snippets", "context-pack"],
                },
                max_calls=7,
            )
            self.assertFalse(packet.claimability["graph_proof"])
            self.assertIn(packet.claimability["proof_status"], {"no_proof_path_found", "no_evidence_found"})
            result = new_result(
                run_id="r",
                task_id="t",
                benchmark="internal",
                track="diagnostic",
                mode="codegraph_planned",
                context_provider="codegraph_planned",
            )
            self.assertEqual(validate_result(result), [])

    def test_graph_proof_and_candidate_only_ladder(self):
        text_item = normalize_codegraph_evidence_item(
            {"file": "src/lib.rs", "graph_proof": True, "proof_status": "verified"},
            role="text_evidence",
            source="unit",
        )
        self.assertFalse(text_item["graph_proof"])
        self.assertEqual(text_item["proof_strength"], "text_evidence")
        graph_item = normalize_codegraph_evidence_item(
            {"file": "src/lib.rs", "graph_proof": True, "proof_status": "verified"},
            role="graph_relation_proof",
            source="unit",
        )
        self.assertTrue(graph_item["graph_proof"])
        self.assertEqual(graph_item["proof_strength"], "graph_relation_proof")
        vector_item = normalize_codegraph_evidence_item(
            {"file": "src/lib.rs", "candidate_source": "vector_semantic", "graph_proof": True, "proof_status": "verified"},
            role="candidate_evidence",
            source="unit",
        )
        self.assertFalse(vector_item["graph_proof"])
        self.assertTrue(vector_item["candidate_only"])
        self.assertEqual(vector_item["proof_strength"], "candidate_evidence")

    def test_task_no_proof_boundary_downgrades_graph_packet_claimability(self):
        graph_item = normalize_codegraph_evidence_item(
            {"file": "src/lib.rs", "graph_proof": True, "proof_status": "verified"},
            role="graph_relation_proof",
            source="unit",
        )
        claim = _packet_claimability(
            {},
            [graph_item],
            {
                "expected_claimability": {
                    "graph_proof_allowed": False,
                    "expected_proof_status": "no_proof_path_found",
                },
                "forbidden_claim_types": ["graph_proof"],
            },
        )
        self.assertFalse(claim["graph_proof"])
        self.assertEqual(claim["proof_status"], "no_proof_path_found")
        self.assertEqual(claim["proof_strength"], "source_navigation_evidence")

    def test_timing_buckets_and_prebuilt_db_path_are_recorded(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            workspace = root / "workspace"
            _write(repo, "src/lib.rs", "pub fn TargetSymbol() {}\n")
            first = self._run(
                repo,
                workspace,
                {"task": "Find TargetSymbol in src/lib.rs", "visible_query_terms": ["TargetSymbol", "src/lib.rs"]},
                max_calls=6,
            )
            second = self._run(
                repo,
                workspace,
                {"task": "Find TargetSymbol in src/lib.rs", "visible_query_terms": ["TargetSymbol", "src/lib.rs"]},
                max_calls=6,
            )
            self.assertFalse(first.raw["prebuilt_db_used"])
            self.assertTrue(second.raw["prebuilt_db_used"])
            self.assertTrue(str(second.raw["db_path"]).startswith(str(workspace)))
            self.assertFalse(second.raw["normal_dot_codegraph_mutated"])
            self.assertIn("query_subprocess_ms", second.raw["codegraph_subprocess_timing_ms"])
            self.assertIn("context_pack_subprocess_ms", second.raw["codegraph_subprocess_timing_ms"])
            buckets = [
                bucket
                for command in second.raw["commands"]
                for bucket in classify_timing_bucket({"argv": command["argv"], "command_id": command["command_id"]})
            ]
            self.assertIn("codegraph_context_pack_subprocess_ms", buckets)
            self.assertIn("codegraph_query_subprocess_ms", buckets)


def _has_query(argv: list[str], kind: str) -> bool:
    return "query" in argv and kind in argv


if __name__ == "__main__":
    unittest.main()
