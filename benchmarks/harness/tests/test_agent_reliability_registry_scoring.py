import json
import unittest
from pathlib import Path

from benchmarks.harness.agent_reliability_registry import (
    REQUIRED_FIXTURE_TAGS,
    load_agent_reliability_tasks,
    provider_visible_agent_task,
    validate_agent_reliability_registry,
)
from benchmarks.harness.adapters.internal_gold_adapter import InternalGoldAdapter
from benchmarks.harness.config import load_config, validate_config
from benchmarks.harness.scoring.agent_reliability import (
    hallucination_trap,
    patch_outcome_placeholder,
    plan_accuracy,
    proof_discipline,
    routing_packet_quality,
)
from benchmarks.harness.schema import AGENT_RELIABILITY_EVALUATOR_ONLY_FIELDS, new_result, validate_result


REPORT_JSON = Path("reports/audit/agent_reliability_task_registry_scoring.json")


class AgentReliabilityRegistryScoringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tasks = load_agent_reliability_tasks()
        cls.by_id = {task["task_id"]: task for task in cls.tasks}

    def test_task_schema_validation(self):
        result = validate_agent_reliability_registry(self.tasks)
        self.assertEqual(result["status"], "pass", result["errors"])
        self.assertTrue(REQUIRED_FIXTURE_TAGS.issubset(set(result["fixture_tags"])))

    def test_existing_benchmark_configs_tasks_results_still_load(self):
        config_paths = [
            "benchmarks/configs/internal_gold_smoke.toml",
            "benchmarks/configs/internal_gold_full.toml",
            "benchmarks/configs/repobench_smoke.toml",
            "benchmarks/configs/crosscodeeval_smoke.toml",
            "benchmarks/configs/swebench_lite_smoke.toml",
        ]
        for path in config_paths:
            with self.subTest(path=path):
                self.assertEqual(validate_config(load_config(path)), [])

        old_tasks = InternalGoldAdapter(Path("benchmarks/datasets/internal_gold")).load_tasks(limit=1)
        old_v05_tasks = InternalGoldAdapter(Path("benchmarks/datasets/internal_gold_v05/codegraph_v05_internal.jsonl")).load_tasks(limit=1)
        self.assertEqual(len(old_tasks), 1)
        self.assertEqual(len(old_v05_tasks), 1)

        result = new_result(run_id="compat", task_id=old_tasks[0]["task_id"], mode="rg_only")
        self.assertEqual(validate_result(result), [])
        json.dumps(result)

    def test_evaluator_only_fields_are_not_provider_visible(self):
        task = self.by_id["ar_investigation_benchmark_query_leakage"]
        provider_task = provider_visible_agent_task(task)
        self.assertTrue(provider_task["leakage_audit"]["pass"])
        for field in AGENT_RELIABILITY_EVALUATOR_ONLY_FIELDS:
            self.assertNotIn(field, provider_task)
        self.assertNotIn("HiddenGoldTargetSymbol", provider_task["visible_query_terms"])

    def test_scorer_perfect_routing_packet(self):
        task = self.by_id["ar_routing_same_symbol_multiple_files"]
        packet = {
            "task_intent": task["expected_intent"],
            "critical_files": task["expected_critical_files"],
            "critical_symbols": task["expected_critical_symbols"],
            "proof_status": task["expected_proof_status"],
            "proof_ladder": task["expected_proof_ladder"],
            "risks": task["expected_risks"],
            "unknowns": task["expected_unknowns"],
            "validation_steps": task["expected_validation_steps"],
            "follow_up_queries": ["confirm import edge before edit"],
            "edit_plan": [{"file": task["expected_critical_files"][1], "evidence_ref": "import edge"}],
        }
        score = routing_packet_quality(task, packet)
        self.assertEqual(score["normalized_score"], 1.0, score["reasons"])
        json.dumps(score)

    def test_scorer_perfect_plan(self):
        task = self.by_id["ar_plan_config_driven_behavior"]
        plan = {
            "edit_targets": task["expected_critical_files"],
            "symbols": task["expected_critical_symbols"],
            "tests": task["expected_tests"],
            "unknowns": task["expected_unknowns"],
            "validation_steps": task["expected_validation_steps"],
            "plan_facts": task["expected_plan_facts"],
            "claims": ["configuration value must be verified before changing behavior"],
        }
        score = plan_accuracy(task, plan)
        self.assertEqual(score["normalized_score"], 1.0, score["reasons"])
        json.dumps(score)

    def test_scorer_hallucinated_plan(self):
        task = self.by_id["ar_hallucination_stale_docs"]
        output = {
            "edit_targets": ["docs/checkout-discounts.md"],
            "symbols": ["legacyDiscountTable"],
            "claims": ["docs alone prove current runtime behavior"],
            "validation_steps": ["read the stale docs"],
            "confidence": "certain",
            "stale_evidence_used": True,
        }
        score = hallucination_trap(task, output)
        self.assertLess(score["normalized_score"], 1.0)
        self.assertGreater(len(score["reasons"]), 0)

    def test_scorer_forbidden_file_hit(self):
        task = self.by_id["ar_routing_same_symbol_multiple_files"]
        packet = {
            "task_intent": task["expected_intent"],
            "critical_files": task["expected_critical_files"] + task["forbidden_files"],
            "critical_symbols": task["expected_critical_symbols"],
            "proof_status": task["expected_proof_status"],
            "proof_ladder": task["expected_proof_ladder"],
            "risks": task["expected_risks"],
            "unknowns": task["expected_unknowns"],
            "validation_steps": task["expected_validation_steps"],
            "follow_up_queries": ["confirm import edge"],
            "edit_plan": [{"file": task["expected_critical_files"][1], "evidence_ref": "import edge"}],
        }
        score = routing_packet_quality(task, packet)
        self.assertLess(score["normalized_score"], 1.0)
        self.assertTrue(any("forbidden_file_hits" in reason for reason in score["reasons"]))

    def test_scorer_forbidden_symbol_hit(self):
        task = self.by_id["ar_hallucination_test_only_mocks"]
        output = {
            "edit_targets": task["expected_critical_files"],
            "symbols": task["expected_critical_symbols"] + task["forbidden_symbols"],
            "claims": [],
            "validation_steps": task["expected_validation_steps"],
            "confidence": "bounded",
            "unknowns": task["expected_unknowns"],
        }
        score = hallucination_trap(task, output)
        self.assertLess(score["normalized_score"], 1.0)
        self.assertTrue(any("nonexistent_symbol_references" in reason for reason in score["reasons"]))

    def test_scorer_unsupported_proof_claim(self):
        task = self.by_id["ar_proof_no_proof_path"]
        output = {
            "proof_status": "proof_path_found",
            "proof_ladder": ["text_evidence"],
            "unknowns": [],
            "claimability": {"graph_proof": True, "proof_strength": "text_evidence"},
            "text_evidence": [{"proof_strength": "text_evidence", "graph_proof": True}],
        }
        score = proof_discipline(task, output)
        self.assertLess(score["normalized_score"], 1.0)
        self.assertTrue(any("unsupported_proof_claims" in reason for reason in score["reasons"]))

    def test_scorer_missing_unknown(self):
        task = self.by_id["ar_proof_dynamic_dispatch"]
        output = {
            "proof_status": task["expected_proof_status"],
            "proof_ladder": task["expected_proof_ladder"],
            "unknowns": [],
        }
        score = proof_discipline(task, output)
        self.assertLess(score["normalized_score"], 1.0)
        self.assertTrue(any("unknowns" in reason for reason in score["reasons"]))

    def test_scorer_correct_no_proof_path_found(self):
        task = self.by_id["ar_proof_no_proof_path"]
        output = {
            "proof_status": "no_proof_path_found",
            "proof_ladder": task["expected_proof_ladder"],
            "graph_proof": False,
            "unknowns": task["expected_unknowns"],
            "claims": ["fallback context can guide inspection but is not graph proof"],
        }
        score = proof_discipline(task, output)
        self.assertEqual(score["normalized_score"], 1.0, score["reasons"])

    def test_scorer_same_symbol_ambiguity(self):
        task = self.by_id["ar_routing_same_symbol_multiple_files"]
        missing_ambiguity = {
            "edit_targets": task["expected_critical_files"],
            "symbols": task["expected_critical_symbols"],
            "validation_steps": task["expected_validation_steps"],
            "confidence": "bounded",
            "unknowns": [],
        }
        failed = hallucination_trap(task, missing_ambiguity)
        self.assertLess(failed["normalized_score"], 1.0)

        acknowledged = {**missing_ambiguity, "unknowns": task["expected_unknowns"], "risks": task["expected_risks"]}
        passed = hallucination_trap(task, acknowledged)
        self.assertEqual(passed["normalized_score"], 1.0, passed["reasons"])

    def test_scorer_stale_docs_trap(self):
        task = self.by_id["ar_hallucination_stale_docs"]
        score = hallucination_trap(
            task,
            {
                "edit_targets": ["docs/checkout-discounts.md"],
                "symbols": ["legacyDiscountTable"],
                "claims": ["docs alone prove current runtime behavior"],
                "validation_steps": ["read docs only"],
                "unknowns": [],
                "confidence": "certain",
                "evidence_state": "stale_docs_only",
            },
        )
        self.assertLess(score["normalized_score"], 1.0)
        self.assertTrue(any("stale_or_foreign" in reason for reason in score["reasons"]))

    def test_scorer_benchmark_leakage_trap(self):
        task = self.by_id["ar_investigation_benchmark_query_leakage"]
        score = hallucination_trap(
            task,
            {
                "edit_targets": task["expected_critical_files"],
                "symbols": task["expected_critical_symbols"],
                "validation_steps": task["expected_validation_steps"],
                "unknowns": task["expected_unknowns"],
                "uses_hidden_gold_terms": True,
            },
        )
        self.assertLess(score["normalized_score"], 1.0)
        self.assertTrue(any("benchmark_query_leakage" in reason for reason in score["reasons"]))

    def test_scorer_wrapper_exit_code_propagation_fixture(self):
        task = self.by_id["ar_investigation_wrapper_exit_code_propagation"]
        missing_exit_check = {
            "edit_targets": task["expected_critical_files"],
            "symbols": task["expected_critical_symbols"],
            "tests": task["expected_tests"],
            "unknowns": task["expected_unknowns"],
            "validation_steps": ["read wrapper log text"],
            "plan_facts": task["expected_plan_facts"],
        }
        failed = plan_accuracy(task, missing_exit_check)
        self.assertLess(failed["normalized_score"], 1.0)

        with_exit_check = {**missing_exit_check, "validation_steps": task["expected_validation_steps"]}
        passed = plan_accuracy(task, with_exit_check)
        self.assertEqual(passed["normalized_score"], 1.0, passed["reasons"])

    def test_patch_outcome_placeholder_disabled_by_default(self):
        task = self.by_id["ar_patch_outcome_v1_placeholder"]
        result = patch_outcome_placeholder(task)
        self.assertEqual(result["status"], "disabled")
        self.assertFalse(result["enabled"])
        self.assertEqual(result["metrics"]["resolved_percentage"], "not_applicable")

    def test_generated_report_json_contract(self):
        if not REPORT_JSON.exists():
            self.skipTest("audit report JSON is created after implementation")
        data = json.loads(REPORT_JSON.read_text(encoding="utf-8"))
        self.assertEqual(data["status"], "complete")
        self.assertTrue(data["task_registry_created"])
        self.assertTrue(data["scorers_created"])
        self.assertFalse(data["public_claim"])
        self.assertFalse(data["patch_outcome_placeholder_enabled"])


if __name__ == "__main__":
    unittest.main()
