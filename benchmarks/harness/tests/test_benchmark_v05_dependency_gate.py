import json
import unittest
from pathlib import Path

from benchmarks.harness.benchmark_v05_dependency_gate import (
    DEPENDENCY_MANIFEST_SCHEMA_VERSION,
    check_required_artifacts,
    load_dependency_manifest,
    validate_final_gate_completion,
)


MANIFEST_PATH = Path("benchmarks/harness/benchmark_v05_dependency_manifest.json")
INVESTIGATION_GATE_JSON = Path("reports/audit/investigation_plan_quality_fixture_gate.json")
TEST_OUTPUT_DIR = Path("reports/audit/artifacts/benchmark_v05_dependency_reconciliation/test_outputs")


class BenchmarkV05DependencyGateTests(unittest.TestCase):
    def test_manifest_valid_and_current_hard_artifacts_are_explicitly_classified(self):
        manifest = load_dependency_manifest(MANIFEST_PATH)
        self.assertEqual(manifest["schema_version"], DEPENDENCY_MANIFEST_SCHEMA_VERSION)
        self.assertFalse(manifest["public_claim"])
        self.assertFalse(manifest["real_patch_quality_claim"])

        preflight = check_required_artifacts(MANIFEST_PATH)
        self.assertIn(preflight["status"], {"pass", "blocked"})
        for missing in preflight["missing_artifacts"]:
            self.assertTrue(missing["producing_phase"])
            self.assertTrue(missing["repair_action"])

    def test_final_gate_required_artifacts_are_listed_and_have_producers(self):
        manifest = load_dependency_manifest(MANIFEST_PATH)
        entries = {entry["required_artifact_path"]: entry for entry in manifest["artifacts"]}
        for required_path in manifest["final_gate_required_artifacts"]:
            with self.subTest(required_path=required_path):
                self.assertIn(required_path, entries)
                self.assertTrue(entries[required_path]["producing_phase"])
                self.assertTrue(entries[required_path]["repair_action"])

    def test_missing_artifact_without_repair_action_blocks(self):
        manifest = {
            "schema_version": DEPENDENCY_MANIFEST_SCHEMA_VERSION,
            "final_gate_required_artifacts": ["reports/audit/missing_without_repair.json"],
            "artifacts": [
                {
                    "required_artifact_path": "reports/audit/missing_without_repair.json",
                    "required_artifact_kind": "json",
                    "producing_phase": "missing_test_phase",
                    "producing_prompt_name": "missing test prompt",
                    "expected_status_field": "status",
                    "expected_ready_field": "ready_to_move_on",
                    "blocking_if_missing": True,
                    "substitute_artifacts": [],
                    "repair_action": "",
                }
            ],
        }
        path = _write_test_manifest("missing_without_repair.json", manifest)
        preflight = check_required_artifacts(path)
        self.assertEqual(preflight["status"], "blocked")
        self.assertTrue(any("missing and has no repair_action" in error for error in preflight["errors"]))

    def test_unlisted_final_gate_required_artifact_blocks(self):
        manifest = {
            "schema_version": DEPENDENCY_MANIFEST_SCHEMA_VERSION,
            "final_gate_required_artifacts": ["reports/audit/unlisted_required.json"],
            "artifacts": [],
        }
        path = _write_test_manifest("unlisted_required.json", manifest)
        preflight = check_required_artifacts(path)
        self.assertEqual(preflight["status"], "blocked")
        self.assertTrue(any("not listed in the dependency manifest" in error for error in preflight["errors"]))

    def test_required_artifact_without_producing_phase_blocks(self):
        manifest = {
            "schema_version": DEPENDENCY_MANIFEST_SCHEMA_VERSION,
            "final_gate_required_artifacts": ["reports/audit/missing_with_repair.json"],
            "artifacts": [
                {
                    "required_artifact_path": "reports/audit/missing_with_repair.json",
                    "required_artifact_kind": "json",
                    "producing_phase": "",
                    "producing_prompt_name": "",
                    "expected_status_field": "status",
                    "expected_ready_field": "ready_to_move_on",
                    "blocking_if_missing": True,
                    "substitute_artifacts": [],
                    "repair_action": "rerun missing test prompt",
                }
            ],
        }
        path = _write_test_manifest("missing_without_producer.json", manifest)
        preflight = check_required_artifacts(path)
        self.assertEqual(preflight["status"], "blocked")
        self.assertTrue(any("has no producing_phase" in error for error in preflight["errors"]))

    def test_final_gate_cannot_complete_with_blocked_preflight(self):
        preflight = {
            "status": "blocked",
            "missing_artifacts": [{"required_artifact_path": "reports/audit/missing.json"}],
        }
        errors = validate_final_gate_completion(preflight, {"status": "complete"})
        self.assertTrue(errors)

    def test_normalized_investigation_gate_cites_sources_and_validates(self):
        data = json.loads(INVESTIGATION_GATE_JSON.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "complete")
        self.assertTrue(data["ready_to_move_on"])
        self.assertEqual(data["compatibility_mode"], "normalized_from_existing_artifacts")
        self.assertTrue(data["source_artifacts"])
        self.assertTrue(data["investigation_fixture_gate_complete"])
        self.assertFalse(data["public_claim"])
        self.assertFalse(data["real_patch_quality_claim"])
        for fixture in data["fixtures"]:
            with self.subTest(fixture=fixture["fixture"]):
                self.assertTrue(fixture["source_artifacts"])
                self.assertIn("no_graph_proof_overclaim", fixture)


def _write_test_manifest(name: str, data: dict) -> Path:
    TEST_OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    path = TEST_OUTPUT_DIR / name
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")
    return path


if __name__ == "__main__":
    unittest.main()
