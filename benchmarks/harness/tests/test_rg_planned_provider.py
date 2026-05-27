import json
import tempfile
import unittest
from pathlib import Path

from benchmarks.harness.context_providers.base import ProviderBudget
from benchmarks.harness.context_providers.rg_planned import RgPlannedProvider
from benchmarks.harness.task_sanitizer import sanitize_provider_task


def _write(root: Path, rel: str, text: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


class RgPlannedProviderTests(unittest.TestCase):
    def _run(self, repo: Path, task: dict, *, max_calls: int = 10, max_bytes: int = 20000):
        workspace = repo / ".bench-workspace"
        provider = RgPlannedProvider(Path.cwd(), workspace)
        task = {"task_id": "t", "repo_path": str(repo), **task}
        return provider.get_context(task, ProviderBudget(max_tool_calls=max_calls, max_context_bytes=max_bytes, max_time_s=10))

    def test_uses_files_shortlist_and_line_evidence_rg_shapes(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _write(repo, "src/lib.rs", "fn target_symbol() {}\n")
            packet = self._run(repo, {"task": "Find target_symbol", "query_terms": ["target_symbol"]})
            argvs = [command["argv"] for command in packet.raw["plan"] if not command.get("skipped")]
            flattened = [" ".join(argv) for argv in argvs]
            self.assertTrue(any("--files" in argv for argv in argvs))
            self.assertTrue(any("-l" in argv for argv in argvs))
            self.assertTrue(any("-n" in argv for argv in argvs))
            self.assertTrue(any("target_symbol" in command for command in flattened))
            self.assertTrue(all(isinstance(argv, list) for argv in argvs))
            self.assertTrue(all("--" in argv for argv in argvs if "-l" in argv or "-n" in argv))
            self.assertFalse(any(" ".join(argv).startswith("cmd /c") for argv in argvs))
            log_path = Path(packet.raw["command_log_path"])
            self.assertTrue(log_path.exists())
            parsed = [json.loads(line) for line in log_path.read_text(encoding="utf-8").splitlines()]
            self.assertEqual(len(parsed), packet.tool_calls)

    def test_path_like_clue_finds_path_name_without_content_match(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _write(repo, "src/hidden/registry.py", "VALUE = 1\n")
            packet = self._run(
                repo,
                {
                    "task": "Open src/hidden/registry.py before editing the registration flow",
                    "query_terms": ["src/hidden/registry.py"],
                },
            )
            self.assertEqual(packet.files[0], "src/hidden/registry.py")
            self.assertTrue(packet.raw["file_discovery"]["path_name_hits"])

    def test_ignored_benchmark_workspace_repo_is_still_searchable(self):
        ignored_parent = Path("benchmarks/workspaces")
        ignored_parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ignored_parent) as tmp:
            repo = Path(tmp)
            _write(repo, "src/ignored_workspace_target.py", "def ignored_workspace_target(): pass\n")
            packet = self._run(
                repo,
                {
                    "task": "Find src/ignored_workspace_target.py",
                    "query_terms": ["src/ignored_workspace_target.py", "ignored_workspace_target"],
                },
            )
            self.assertIn("src/ignored_workspace_target.py", packet.files)
            argvs = [command["argv"] for command in packet.raw["plan"] if not command.get("skipped")]
            self.assertTrue(all("--no-ignore" in argv for argv in argvs))

    def test_broad_query_flood_is_capped_and_labeled(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            for index in range(30):
                _write(repo, f"pkg/file_{index}.txt", "the common token appears here\n")
            packet = self._run(
                repo,
                {"task": "Find the common token", "query_terms": ["the"]},
                max_calls=4,
                max_bytes=1000,
            )
            events = packet.raw["flood_diagnostics"]["events"]
            self.assertTrue(events)
            self.assertLessEqual(packet.tool_calls, 4)
            self.assertLessEqual(len(packet.files), packet.raw["flood_diagnostics"]["max_files"])
            self.assertLessEqual(len(packet.snippets), packet.raw["flood_diagnostics"]["max_lines"])

    def test_dedupe_removes_repeated_file_line_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _write(repo, "src/lib.rs", "let alpha = alpha + 1;\n")
            packet = self._run(repo, {"task": "Find alpha", "query_terms": ["alpha", "alpha"]})
            keys = [(item["file"], item["line"], item["text"]) for item in packet.snippets]
            self.assertEqual(len(keys), len(set(keys)))

    def test_hidden_gold_terms_do_not_reach_rg_planned(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _write(repo, "src/public.py", "def public_symbol(): pass\n")
            _write(repo, "src/secret_gold.py", "def SecretSymbol(): pass\n")
            task = sanitize_provider_task(
                {
                    "task_id": "t",
                    "repo_path": str(repo),
                    "task": "Find the public entry point",
                    "query_terms": ["src/secret_gold.py", "SecretSymbol"],
                    "visible_query_terms": ["public_symbol"],
                    "gold_files": ["src/secret_gold.py"],
                    "gold_symbols": ["SecretSymbol"],
                    "gold_spans": [],
                }
            )
            packet = RgPlannedProvider(Path.cwd(), repo / ".bench-workspace").get_context(
                task, ProviderBudget(max_tool_calls=6, max_context_bytes=20000, max_time_s=10)
            )
            serialized_plan = json.dumps(packet.raw["plan"])
            self.assertTrue(task["leakage_audit"]["pass"])
            self.assertIn("public_symbol", serialized_plan)
            self.assertNotIn("secret_gold.py", serialized_plan)
            self.assertNotIn("SecretSymbol", serialized_plan)

    def test_exact_filename_query_ranks_filename_hit_high(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _write(repo, "docs/manual/adding-packages.adoc", "manual body\n")
            _write(repo, "docs/manual/other.adoc", "adding-packages.adoc mentioned here\n")
            packet = self._run(
                repo,
                {"task": "Find adding-packages.adoc", "query_terms": ["adding-packages.adoc"]},
            )
            self.assertEqual(packet.files[0], "docs/manual/adding-packages.adoc")

    def test_config_doc_task_uses_path_extension_aware_search(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _write(repo, "package/foo/Config.in", "config BR2_PACKAGE_FOO\n")
            _write(repo, "package/foo/foo.mk", "FOO_SITE = https://example.invalid\n")
            packet = self._run(
                repo,
                {
                    "task": "Find the Config.in option for BR2_PACKAGE_FOO",
                    "query_terms": ["Config.in", "BR2_PACKAGE_FOO"],
                },
            )
            self.assertEqual(packet.files[0], "package/foo/Config.in")
            self.assertTrue(
                any(
                    any("Config.in" in value for value in command.get("path_filters", []))
                    for command in packet.raw["plan"]
                )
            )

    def test_implementation_trace_extracts_symbol_path_accounting_clues(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _write(repo, "crates/codegraph-vector/src/lib.rs", "let estimated_raw_f32 = chunk_count * 4;\n")
            packet = self._run(
                repo,
                {
                    "task": "Trace vector chunk accounting in crates/codegraph-vector/src/lib.rs for estimated_raw_f32",
                    "task_type": "implementation_trace",
                    "query_terms": ["vector chunk", "estimated_raw_f32"],
                },
            )
            clues = packet.raw["plan"][0]["clues"]
            self.assertIn("estimated_raw_f32", [item["term"] for item in clues["search_terms"]])
            self.assertTrue(any(item.get("query_source") == "implementation_trace" for item in packet.raw["plan"]))
            self.assertIn("crates/codegraph-vector/src/lib.rs", packet.files[:3])

    def test_output_schema_counts_and_claim_boundary(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            _write(repo, "src/lib.rs", "fn beta() {}\n")
            packet = self._run(repo, {"task": "Find beta", "query_terms": ["beta"]})
            payload = packet.to_dict()
            self.assertIn("files", payload)
            self.assertIn("spans", payload)
            self.assertIn("snippets", payload)
            self.assertIn("raw", payload)
            self.assertEqual(packet.tool_calls, len([command for command in packet.raw["plan"] if not command.get("skipped")]))
            self.assertEqual(packet.raw_context_bytes, packet.raw["context_payload"]["bytes"])
            self.assertEqual(packet.raw["metrics"]["tool_calls"], packet.tool_calls)
            self.assertIn("command_wall_time_ms", packet.raw["metrics"])
            self.assertGreaterEqual(packet.raw["metrics"]["command_wall_time_ms"], 0)
            self.assertGreaterEqual(packet.raw["metrics"]["stdout_bytes_total"], 0)
            self.assertFalse(packet.claimability["graph_proof"])
            self.assertEqual(packet.claimability["evidence_role"], "lexical/text/path evidence")


if __name__ == "__main__":
    unittest.main()
