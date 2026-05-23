import unittest
from pathlib import Path

from benchmarks.harness.quality_metrics import quality_per_budget
from benchmarks.harness.runners.run_benchmark_suite import (
    _blocked_tracks_from_plan,
    build_run_plan,
)
from benchmarks.harness.timing import classify_timing_bucket, timing_breakdown
from benchmarks.harness.track_status import (
    TrackStatus,
    classify_swe_readiness,
    validate_track_status,
)


def _fake_preflight():
    return {
        "release_binary": {"exists": True, "version": "codegraph-mcp 0.0.0 (unknown)"},
        "dirty_worktree_snapshot": {"branch": "fix", "commit": "abc", "status_short": []},
        "normal_dot_codegraph_before_run": False,
        "dataset_readiness": {
            "internal_gold": {"status": TrackStatus.READY_FOR_FULL_RUN.value, "task_count": 20},
            "repobench": {"status": TrackStatus.READY_FOR_FULL_RUN.value},
            "crosscodeeval": {"status": TrackStatus.READY_FOR_OFFICIAL_SMOKE.value},
            "swe_bench": {
                "gold_validation_cached": {
                    "status": TrackStatus.GOLD_VALIDATION_CACHED_READY.value,
                    "counts_as_current_live_run": False,
                },
                "gold_validation_live": {
                    "status": TrackStatus.BLOCKED_DOCKER_LIVE_RUN.value,
                    "blocked_reason": "docker unavailable",
                },
                "patch_quality": {
                    "status": TrackStatus.PATCH_QUALITY_BLOCKED_MISSING_EXTERNAL_AGENT.value,
                    "blocked_reason": "missing external agent",
                },
            },
        },
        "ignored_output_paths": {"ignored": True},
        "claim_boundary": "local diagnostic only",
    }


class OperatorGradeBenchmarkSuiteTests(unittest.TestCase):
    def test_track_status_model_contains_required_values(self):
        for value in [
            "ready_for_full_run",
            "ready_for_retrieval_full_run",
            "ready_for_official_smoke",
            "gold_validation_cached_ready",
            "gold_validation_live_ready",
            "patch_quality_blocked_missing_external_agent",
            "patch_quality_blocked_docker",
            "blocked_docker_live_run",
            "blocked_query_leakage",
            "not_configured_by_user",
            "scaffold_only",
            "unavailable_missing_dataset",
            "unavailable_missing_binary",
            "timed_out",
            "completed",
            "failed",
            "official_score_not_claimed",
        ]:
            self.assertTrue(validate_track_status(value), value)

    def test_swe_readiness_splits_cached_live_and_patch_quality(self):
        swe = {
            "smoke_ready": True,
            "details": {
                "docker": {"status": "blocked_docker", "blocker": "pipe denied"},
                "python_import": {"status": "ready"},
                "linux_harness": {
                    "status": "ready",
                    "gold_validation_report": "benchmarks/workspaces/swebench_gold_validation/gold.json",
                },
            },
        }
        patch = {"external_agent_command": {"status": "blocked_not_configured", "env": "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND"}}
        result = classify_swe_readiness(swe, patch)
        self.assertEqual(result["gold_validation_cached"]["status"], TrackStatus.GOLD_VALIDATION_CACHED_READY.value)
        self.assertFalse(result["gold_validation_cached"]["counts_as_current_live_run"])
        self.assertEqual(result["gold_validation_live"]["status"], TrackStatus.BLOCKED_DOCKER_LIVE_RUN.value)
        self.assertEqual(
            result["patch_quality"]["status"],
            TrackStatus.PATCH_QUALITY_BLOCKED_MISSING_EXTERNAL_AGENT.value,
        )

    def test_swe_patch_quality_blocks_on_docker_when_agent_ready(self):
        swe = {
            "smoke_ready": True,
            "details": {
                "docker": {"status": "blocked_docker", "blocker": "pipe denied"},
                "python_import": {"status": "ready"},
                "linux_harness": {},
            },
        }
        patch = {"external_agent_command": {"status": "ready", "env": "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND"}}
        result = classify_swe_readiness(swe, patch)
        self.assertEqual(result["patch_quality"]["status"], TrackStatus.PATCH_QUALITY_BLOCKED_DOCKER.value)

    def test_run_plan_generation_includes_smoke_and_blocked_tracks(self):
        plan = build_run_plan("smoke", Path("benchmarks/results/summaries/test_plan"), _fake_preflight())
        names = {track["name"] for track in plan["tracks"]}
        self.assertIn("unit_provider_smoke", names)
        self.assertIn("setup_verification_smoke", names)
        self.assertIn("internal_gold_smoke", names)
        self.assertIn("v0_scoring_diagnostics", names)
        self.assertIn("repobench_readiness", names)
        self.assertIn("crosscodeeval_official_smoke_readiness", names)
        self.assertIn("swebench_patch_quality", names)
        self.assertFalse(plan["public_claim"])

    def test_retrieval_plan_has_prebuild_setup_phase_for_codegraph_modes(self):
        preflight = _fake_preflight()
        preflight["dataset_readiness"]["repobench"] = {"status": TrackStatus.UNAVAILABLE_MISSING_DATASET.value}
        preflight["dataset_readiness"]["crosscodeeval"] = {"status": TrackStatus.UNAVAILABLE_MISSING_DATASET.value}
        plan = build_run_plan("retrieval", Path("benchmarks/results/summaries/test_retrieval_plan"), preflight)
        prebuilds = [track for track in plan["tracks"] if track["kind"] == "prebuild"]
        self.assertTrue(prebuilds)
        self.assertTrue(prebuilds[0]["cold_setup_recorded_separately"])
        self.assertIn("codegraph_full", prebuilds[0]["provider_mode"])

    def test_v05_external_plan_uses_external_four_mode_configs_and_blocks_swe(self):
        plan = build_run_plan("v0.5-external", Path("benchmarks/results/summaries/test_v05_external_plan"), _fake_preflight())
        tracks = {track["name"]: track for track in plan["tracks"]}
        self.assertIn("repobench_v05_external", tracks)
        self.assertIn("crosscodeeval_v05_external", tracks)
        self.assertEqual(
            tracks["repobench_v05_external"]["provider_mode"],
            "rg_only,rg_planned,codegraph_current,codegraph_planned",
        )
        self.assertEqual(tracks["swebench_live_gold_validation"]["status"], TrackStatus.BLOCKED_DOCKER_LIVE_RUN.value)
        self.assertEqual(
            tracks["swebench_patch_quality"]["status"],
            TrackStatus.PATCH_QUALITY_BLOCKED_MISSING_EXTERNAL_AGENT.value,
        )
        self.assertFalse(plan["public_claim"])

    def test_blocked_tracks_generation(self):
        plan = build_run_plan("smoke", Path("benchmarks/results/summaries/test_blocked"), _fake_preflight())
        blocked = _blocked_tracks_from_plan(plan)
        names = {track["track"] for track in blocked}
        self.assertIn("swebench_patch_quality", names)
        self.assertIn("candidate_spool_diagnostics", names)

    def test_timing_bucket_classification(self):
        record = {"argv": ["target/release/codegraph-mcp.exe", "context-pack"], "command_id": "ctx", "wall_time_ms": 50}
        self.assertIn("codegraph_context_pack_subprocess_ms", classify_timing_bucket(record))
        breakdown = timing_breakdown([record])
        self.assertEqual(breakdown["buckets"]["codegraph_context_pack_subprocess_ms"], 50)
        self.assertEqual(breakdown["buckets"]["warm_context_pack_ms"], 50)

    def test_quality_per_budget_metrics(self):
        results = [
            {
                "mode": "rg_only",
                "retrieval": {
                    "gold_file_recall_at_5": 0.5,
                    "mrr": 0.25,
                    "context_tokens_estimated": 1000,
                    "context_bytes": 4000,
                    "gold_file_count": 2,
                },
                "efficiency": {"tool_calls": 2, "wall_time_ms": 1000},
            }
        ]
        metrics = quality_per_budget(results)["modes"]["rg_only"]
        self.assertEqual(metrics["recall_per_1k_tokens"], 0.5)
        self.assertEqual(metrics["mrr_per_tool_call"], 0.125)
        self.assertEqual(metrics["files_recalled_per_second_warm"], 1.0)


if __name__ == "__main__":
    unittest.main()
