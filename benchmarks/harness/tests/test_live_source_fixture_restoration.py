import json
import re
import subprocess
import sys
import unittest
from pathlib import Path

from benchmarks.harness.config import load_config, validate_config
from benchmarks.harness.runners.run_benchmark_suite import suite_inventory
from benchmarks.harness.runners.verify_benchmark_setup import CONFIGS


ROOT = Path(__file__).resolve().parents[3]
GRAPH_TRUTH_FIXTURES = ROOT / "benchmarks" / "tracks" / "graph_truth" / "fixtures"
EXPECTED_GRAPH_TRUTH_CASES = {
    "admin_user_middleware_role_separation",
    "barrel_export_default_export_resolution",
    "derived_closure_edge_requires_provenance",
    "dynamic_import_marked_heuristic",
    "file_rename_prunes_old_path",
    "import_alias_change_updates_target",
    "same_function_name_only_one_imported",
    "sanitizer_exists_but_not_on_flow",
    "source_span_exact_callsite",
    "stale_graph_cache_after_edit_delete",
    "test_mock_not_production_call",
}


class LiveSourceFixtureRestorationTests(unittest.TestCase):
    def test_benchmark_source_inventory_is_live_source_not_pycache_only(self):
        required_source = [
            "benchmarks/harness/runners/run_benchmark_suite.py",
            "benchmarks/harness/runners/verify_benchmark_setup.py",
            "benchmarks/harness/config.py",
            "benchmarks/harness/context_providers/rg_planned.py",
            "benchmarks/harness/context_providers/codegraph_planned.py",
            "benchmarks/harness/scoring/claimability.py",
            "benchmarks/harness/scoring/context_recall.py",
            "benchmarks/harness/scoring/evidence_alignment.py",
            "benchmarks/harness/tests/test_query_leakage_and_poison_metrics.py",
            "benchmarks/scripts/run_codex_external_patch_agent.ps1",
            "benchmarks/upstream/pinned_sources.json",
        ]
        for relative in required_source:
            with self.subTest(path=relative):
                path = ROOT / relative
                self.assertTrue(path.exists(), relative)
                self.assertNotIn("branch_split", path.as_posix())

        py_sources = list((ROOT / "benchmarks" / "harness").rglob("*.py"))
        self.assertGreaterEqual(len(py_sources), 50)

    def test_run_benchmark_suite_import_and_list_suites_cli(self):
        inventory = suite_inventory()
        self.assertIn("full", inventory["suites"])
        self.assertFalse("public benchmark" in inventory["claim_boundary"].lower())

        proc = subprocess.run(
            [sys.executable, "-m", "benchmarks.harness.runners.run_benchmark_suite", "--list-suites"],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        payload = json.loads(proc.stdout)
        self.assertEqual(payload["schema_version"], "benchmark_suite_inventory_v1")
        self.assertIn("smoke", payload["suites"])
        self.assertIn("full", payload["suites"])

    def test_configs_parse_from_canonical_and_compat_paths(self):
        for config_path in CONFIGS:
            with self.subTest(config=config_path):
                path = ROOT / config_path
                self.assertTrue(path.exists(), config_path)
                errors = validate_config(load_config(path))
                self.assertEqual(errors, [], f"{config_path}: {errors}")

    def test_task_manifests_parse(self):
        manifest_paths = [
            ROOT / "benchmarks" / "datasets" / "v1_readiness" / "task_manifest.template.json",
            ROOT / "benchmarks" / "datasets" / "v1_real_agent_patch_outcomes" / "task_registry.json",
            ROOT / "benchmarks" / "datasets" / "agent_reliability" / "agent_reliability_tasks.jsonl",
            ROOT / "benchmarks" / "datasets" / "internal_gold_v05" / "codegraph_v05_internal.jsonl",
        ]
        for path in manifest_paths[:2]:
            with self.subTest(path=path):
                self.assertTrue(path.exists(), str(path))
                payload = json.loads(path.read_text(encoding="utf-8-sig"))
                self.assertFalse(payload.get("public_claim", False))
        jsonl_path = manifest_paths[2]
        rows = [json.loads(line) for line in jsonl_path.read_text(encoding="utf-8-sig").splitlines() if line.strip()]
        self.assertGreater(len(rows), 0)
        v05_rows = [
            json.loads(line)
            for line in manifest_paths[3].read_text(encoding="utf-8-sig").splitlines()
            if line.strip()
        ]
        self.assertGreater(len(v05_rows), 0)
        for row in v05_rows:
            repo_path = row.get("repo_path")
            with self.subTest(task=row.get("task_id"), repo_path=repo_path):
                self.assertTrue(repo_path)
                self.assertTrue((ROOT / repo_path).exists(), repo_path)
                if row.get("repo_kind") == "graph_truth_fixture":
                    self.assertIn("benchmarks/tracks/graph_truth/fixtures/", repo_path)

    def test_current_graph_truth_fixtures_are_discoverable(self):
        manifests = sorted(GRAPH_TRUTH_FIXTURES.glob("*/graph_truth_case.json"))
        case_ids = {path.parent.name for path in manifests}
        self.assertEqual(case_ids, EXPECTED_GRAPH_TRUTH_CASES)
        for manifest in manifests:
            with self.subTest(case=manifest.parent.name):
                payload = json.loads(manifest.read_text(encoding="utf-8-sig"))
                self.assertEqual(payload["case_id"], manifest.parent.name)
                self.assertEqual(
                    payload["repo_fixture_path"],
                    f"benchmarks/tracks/graph_truth/fixtures/{manifest.parent.name}/repo",
                )
                self.assertNotIn("branch_split", payload["repo_fixture_path"])
                self.assertTrue((manifest.parent / "README.md").exists())
                self.assertTrue(any((manifest.parent / "repo").rglob("*.ts")))

    def test_release_comprehensive_fixture_is_discoverable(self):
        basic_repo = ROOT / "fixtures" / "smoke" / "basic_repo"
        self.assertTrue((basic_repo / "README.md").exists())
        self.assertTrue((basic_repo / "src" / "smoke.ts").exists())
        self.assertTrue((ROOT / "target" / "release" / "codegraph-mcp.exe").exists())

    def test_claim_boundary_fixture_sources_are_discoverable(self):
        claims_doc = ROOT / "benchmarks" / "BENCHMARK_CLAIMS.md"
        text = claims_doc.read_text(encoding="utf-8")
        self.assertIn("CodeGraph beats rg", text)
        self.assertIn("Mock-agent", text)
        self.assertIn("model quality", text)
        self.assertTrue((ROOT / "benchmarks" / "configs" / "benchmark_v1_readiness.template.toml").exists())
        self.assertTrue((ROOT / "benchmarks" / "scripts" / "run_codex_external_patch_agent.ps1").exists())

    def test_leakage_precision_context_poison_sources_are_discoverable(self):
        self.assertTrue((ROOT / "benchmarks" / "harness" / "tests" / "test_query_leakage_and_poison_metrics.py").exists())
        fixtures = ROOT / "benchmarks" / "fixtures" / "v1_real_agent_patch_outcomes" / "local_patch_fixtures"
        for fixture in [
            "local_wrong_file_trap_discount",
            "local_same_name_ambiguity_tenant",
            "local_test_mock_leakage_mailer",
            "local_stale_docs_trap_worker",
        ]:
            with self.subTest(fixture=fixture):
                self.assertTrue((fixtures / fixture / "repo").exists())

    def test_no_generated_raw_artifacts_are_tracked_or_staged(self):
        forbidden = re.compile(
            r"(^|/)(results|workspaces)(/|$)|"
            r"(^|/)upstream/(SWE-bench|repobench|cceval)(/|$)|"
            r"(^|/)\.codegraph(/|$)|"
            r"\.(sqlite|sqlite-wal|sqlite-shm|db|wal|shm|pyc)$",
            re.IGNORECASE,
        )
        tracked = _git_lines(["git", "ls-files", "--", "benchmarks"])
        staged = _git_lines(["git", "diff", "--cached", "--name-only", "--", "benchmarks"])
        leaked = [path for path in tracked + staged if forbidden.search(path.replace("\\", "/"))]
        self.assertEqual(leaked, [])
        self.assertFalse((ROOT / "benchmarks" / "tracks" / "swebench_lite" / "upstream" / "SWE-bench").exists())
        for ignored_path in [
            "benchmarks/tracks/internal_gold/results/source_fixture_probe",
            "benchmarks/tracks/repobench/workspaces/source_fixture_probe",
            "benchmarks/tracks/swebench_lite/upstream/SWE-bench",
        ]:
            with self.subTest(ignored_path=ignored_path):
                proc = subprocess.run(
                    ["git", "check-ignore", ignored_path],
                    cwd=ROOT,
                    text=True,
                    capture_output=True,
                    check=False,
                )
                self.assertEqual(proc.returncode, 0, proc.stderr)


def _git_lines(argv: list[str]) -> list[str]:
    proc = subprocess.run(argv, cwd=ROOT, text=True, capture_output=True, check=False)
    if proc.returncode != 0:
        raise AssertionError(f"{' '.join(argv)} failed: {proc.stderr}")
    return [line.strip() for line in proc.stdout.splitlines() if line.strip()]


if __name__ == "__main__":
    unittest.main()
