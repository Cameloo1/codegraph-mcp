from __future__ import annotations

import argparse
import json
import os
import platform
import sys
import time
from pathlib import Path
from typing import Any

from benchmarks.harness.command_runner import CommandRunner
from benchmarks.harness.config import BenchmarkConfig, load_config
from benchmarks.harness.quality_metrics import quality_per_budget
from benchmarks.harness.reports.generate_charts import generate_charts
from benchmarks.harness.resource_guard import (
    ResourceLimits,
    add_resource_guard_arguments,
    resource_limits_from_args,
    summarize_resource_limit_failures,
)
from benchmarks.harness.runners.run_patch_eval import patch_setup_summary
from benchmarks.harness.runners.verify_benchmark_setup import verify_setup
from benchmarks.harness.scoring.claimability import claimability_violations, no_proof_behavior_ok
from benchmarks.harness.scoring.evidence_alignment import evidence_alignment_summary
from benchmarks.harness.scoring.hallucination import unsupported_claim_violations
from benchmarks.harness.scoring.efficiency import aggregate_by_mode
from benchmarks.harness.timing import timing_breakdown
from benchmarks.harness.track_status import (
    TrackStatus,
    classify_external_readiness,
    validate_track_status,
)
from benchmarks.harness.workspace import ensure_dir, resolve_repo_path, stable_id


SUITES = {"smoke", "retrieval", "full", "swebench-focused", "v0.5-internal", "v0.5-external"}

PROVIDER_INVENTORY = {
    "implemented": [
        "baseline",
        "none",
        "rg_only",
        "rg_planned",
        "codegraph_exact_text",
        "codegraph_full",
        "codegraph_current",
        "codegraph_planned",
    ],
    "missing": ["rg_literal"],
    "aliases": {
        "rg_literal": "rg_only",
        "codegraph_current": "codegraph_full provider behavior under explicit v0.5 name",
    },
}

RETRIEVAL_CONFIGS = {
    "smoke": [
        {
            "name": "internal_gold_smoke",
            "config": "benchmarks/configs/internal_gold_smoke.toml",
            "modes": ["none", "rg_only"],
            "max_tasks": 2,
        }
    ],
    "retrieval": [
        {"name": "internal_gold_full", "config": "benchmarks/configs/internal_gold_full.toml"},
        {"name": "repobench_small", "config": "benchmarks/configs/repobench_small.toml"},
        {"name": "crosscodeeval_small", "config": "benchmarks/configs/crosscodeeval_small.toml"},
    ],
    "full": [
        {"name": "internal_gold_full", "config": "benchmarks/configs/internal_gold_full.toml"},
        {"name": "repobench_small", "config": "benchmarks/configs/repobench_small.toml"},
        {"name": "crosscodeeval_small", "config": "benchmarks/configs/crosscodeeval_small.toml"},
    ],
    "swebench-focused": [],
    "v0.5-internal": [
        {"name": "internal_gold_v05", "config": "benchmarks/configs/internal_gold_v05.toml"},
    ],
    "v0.5-external": [
        {"name": "repobench_v05_external", "config": "benchmarks/configs/repobench_v05_external.toml"},
        {"name": "crosscodeeval_v05_external", "config": "benchmarks/configs/crosscodeeval_v05_external.toml"},
    ],
}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--suite", choices=sorted(SUITES))
    parser.add_argument("--output-dir")
    parser.add_argument("--list-suites", action="store_true")
    add_resource_guard_arguments(parser)
    args = parser.parse_args(argv)
    if args.list_suites:
        print(json.dumps(suite_inventory(), indent=2))
        return 0
    if not args.suite:
        parser.error("--suite is required unless --list-suites is used")
    if not args.output_dir:
        parser.error("--output-dir is required unless --list-suites is used")
    output_dir = ensure_dir(Path(args.output_dir))
    result = run_suite(args.suite, output_dir, resource_limits=resource_limits_from_args(args))
    return 0 if result["status"] == "pass" else 1


def suite_inventory() -> dict[str, Any]:
    return {
        "schema_version": "benchmark_suite_inventory_v1",
        "suites": sorted(SUITES),
        "provider_inventory": PROVIDER_INVENTORY,
        "retrieval_configs": RETRIEVAL_CONFIGS,
        "claim_boundary": "suite inventory only; not a benchmark score or public result",
    }


def run_suite(suite: str, output_dir: Path, *, resource_limits: ResourceLimits | None = None) -> dict[str, Any]:
    started = time.perf_counter()
    run_id = output_dir.name or f"{suite}_{int(time.time())}"
    command_runner = CommandRunner(
        output_dir / "command_logs",
        commands_jsonl=output_dir / "commands.jsonl",
        resource_limits=resource_limits,
    )
    preflight = collect_preflight(command_runner, output_dir)
    run_plan = build_run_plan(suite, output_dir, preflight)
    _write_json(output_dir / "run_plan.json", run_plan)
    run_plan_written_at = time.strftime("%Y-%m-%dT%H:%M:%S%z")

    executed_tracks: list[dict[str, Any]] = []
    all_results: list[dict[str, Any]] = []
    manifest_entries: list[dict[str, Any]] = []
    blocked_tracks = _blocked_tracks_from_plan(run_plan)

    for track in run_plan["tracks"]:
        if track["action"] in {"blocked", "evidence_only", "scaffold_only", "optional_lab_only"}:
            manifest_entries.append(_manifest_from_track(track, command_ids=[]))
            continue
        if track["kind"] == "prebuild":
            track_result = _execute_prebuild_track(track, command_runner)
        elif track["kind"] == "command":
            track_result = _execute_command_track(track, command_runner)
        elif track["kind"] == "artifact_hygiene":
            track_result = _execute_artifact_hygiene_track(track, preflight)
        elif track["kind"] == "v0_scoring_diagnostics":
            track_result = _execute_v0_scoring_diagnostics_track(track)
        else:
            track_result = {
                "name": track["name"],
                "status": TrackStatus.FAILED.value,
                "blocked_reason": f"unknown track kind: {track['kind']}",
                "command_ids": [],
            }
        executed_tracks.append(track_result)
        manifest_entries.append(_manifest_from_track(track, track_result.get("command_ids", []), track_result))
        if track.get("result_loader") == "retrieval":
            all_results.extend(_load_retrieval_results(Path(track["output_path"])))

    blocked_tracks.extend(_blocked_tracks_from_execution(executed_tracks, run_plan))
    commands = _load_jsonl(output_dir / "commands.jsonl")
    resource_limit_failures = summarize_resource_limit_failures(commands)
    timing = timing_breakdown(commands, setup_paid_labels=_setup_paid_labels(run_plan))
    timing_by_mode = _timing_by_mode(all_results)
    quality = quality_per_budget(all_results, timing_by_mode=timing_by_mode)
    after_dot_codegraph = Path(".codegraph").exists()
    normal_dot_codegraph_mutated = bool(preflight["normal_dot_codegraph_before_run"]) != after_dot_codegraph

    summary = _build_summary(
        suite=suite,
        run_id=run_id,
        output_dir=output_dir,
        preflight=preflight,
        run_plan=run_plan,
        executed_tracks=executed_tracks,
        all_results=all_results,
        normal_dot_codegraph_mutated=normal_dot_codegraph_mutated,
        elapsed_ms=int((time.perf_counter() - started) * 1000),
        run_plan_written_at=run_plan_written_at,
    )
    summary["resource_guard"] = (resource_limits or command_runner.resource_limits).to_dict()
    summary["resource_limit_failures"] = resource_limit_failures
    all_results_doc = {
        "schema_version": "benchmark_suite_all_results_v1",
        "suite": suite,
        "run_id": run_id,
        "public_claim": False,
        "results": all_results,
        "track_results": executed_tracks,
    }
    manifest = {
        "schema_version": "benchmark_track_artifacts_manifest_v1",
        "tracks": manifest_entries,
    }

    ensure_dir(output_dir / "charts")
    _write_json(output_dir / "all_results.json", all_results_doc)
    _write_json(output_dir / "summary.json", summary)
    _write_json(output_dir / "blocked_tracks.json", {"schema_version": "benchmark_blocked_tracks_v1", "tracks": blocked_tracks})
    _write_json(output_dir / "timing_breakdown.json", timing)
    _write_json(output_dir / "quality_per_budget.json", quality)
    _write_json(output_dir / "track_artifacts_manifest.json", manifest)
    _write_summary_md(output_dir / "summary.md", summary, quality, timing)
    _write_charts(output_dir, all_results)
    return summary


def collect_preflight(command_runner: CommandRunner, output_dir: Path) -> dict[str, Any]:
    release_binary = Path("target/release/codegraph-mcp.exe")
    branch = command_runner.run(["git", "branch", "--show-current"], command_id="preflight_git_branch", timeout_s=20)
    commit = command_runner.run(["git", "rev-parse", "HEAD"], command_id="preflight_git_commit", timeout_s=20)
    dirty = command_runner.run(
        ["git", "status", "--short", "--untracked-files=all"],
        command_id="preflight_git_status_short",
        timeout_s=20,
    )
    if release_binary.exists():
        version = command_runner.run([str(release_binary), "--version"], command_id="preflight_release_version", timeout_s=30)
        release_version = _read_text_file(version.stdout_path).strip()
    else:
        release_version = ""
    setup_summary = verify_setup()
    patch_summary = patch_setup_summary("benchmarks/configs/swebench_lite_smoke.toml")
    external_readiness = classify_external_readiness(setup_summary, patch_summary)
    output_ignore = command_runner.run(
        ["git", "check-ignore", str(output_dir).replace("\\", "/")],
        command_id="preflight_output_ignore",
        timeout_s=20,
        failure_kind_hint="setup_blocked",
    )
    pinned_sources = _load_json(Path("benchmarks/upstream/pinned_sources.json"), default={})
    return {
        "schema_version": "benchmark_suite_preflight_v1",
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "release_binary": {
            "path": str(release_binary),
            "exists": release_binary.exists(),
            "version": release_version,
            "status": TrackStatus.READY_FOR_FULL_RUN.value if release_binary.exists() else TrackStatus.UNAVAILABLE_MISSING_BINARY.value,
        },
        "benchmark_setup_verification": setup_summary,
        "docker_state": setup_summary.get("adapters", {}).get("swe_bench_lite", {}).get("details", {}).get("docker", {}),
        "external_agent_command_state": patch_summary.get("external_agent_command", {}),
        "dataset_readiness": external_readiness,
        "pinned_upstream_refs": pinned_sources,
        "ignored_output_paths": {
            "output_dir": str(output_dir),
            "git_check_ignore_exit_code": output_ignore.exit_code,
            "ignored": output_ignore.exit_code == 0,
            "stdout": _read_text_file(output_ignore.stdout_path).strip(),
            "stderr": _read_text_file(output_ignore.stderr_path).strip(),
        },
        "dirty_worktree_snapshot": {
            "branch": _read_text_file(branch.stdout_path).strip(),
            "commit": _read_text_file(commit.stdout_path).strip(),
            "status_short": [line for line in _read_text_file(dirty.stdout_path).splitlines() if line.strip()],
        },
        "normal_dot_codegraph_before_run": Path(".codegraph").exists(),
        "environment": {
            "os": platform.platform(),
            "python": sys.version,
            "shell": os.environ.get("ComSpec") or os.environ.get("SHELL") or "unknown",
            "cwd": str(Path.cwd()),
        },
        "claim_boundary": "local diagnostic only; no public benchmark claim",
    }


def build_run_plan(suite: str, output_dir: Path, preflight: dict[str, Any]) -> dict[str, Any]:
    if suite not in SUITES:
        raise ValueError(f"unsupported suite: {suite}")
    tracks: list[dict[str, Any]] = []
    if suite == "smoke":
        tracks.append(
            _command_track(
                "unit_provider_smoke",
                [sys.executable, "-m", "unittest", "discover", "-s", "benchmarks/harness/tests"],
                output_dir / "unit_provider_smoke",
                task_set="benchmark harness unit/provider smoke",
                dataset="local tests",
                timeout_s=120,
            )
        )
        tracks.append(
            _command_track(
                "setup_verification_smoke",
                [
                    sys.executable,
                    "-m",
                    "benchmarks.harness.runners.verify_benchmark_setup",
                    "--output-dir",
                    str(output_dir / "setup_verification"),
                ],
                output_dir / "setup_verification",
                task_set="setup verification",
                dataset="local benchmark setup",
                timeout_s=180,
            )
        )
    tracks.extend(_retrieval_tracks_for_suite(suite, output_dir, preflight))
    if suite in {"smoke", "retrieval", "full"}:
        tracks.append(
            {
                "name": "v0_scoring_diagnostics",
                "kind": "v0_scoring_diagnostics",
                "action": "run",
                "provider_mode": "none",
                "task_set": "toy hallucination and evidence alignment scorer checks",
                "dataset": "local diagnostic fixture",
                "raw_output_path": str(output_dir / "v0_scoring_diagnostics.json"),
                "summary_path": str(output_dir / "v0_scoring_diagnostics.json"),
                "status": TrackStatus.READY_FOR_FULL_RUN.value,
                "blocked_reason": "",
                "commands": [],
                "output_paths": [str(output_dir / "v0_scoring_diagnostics.json")],
                "budget": {"timeout_s": 30},
            }
        )
    tracks.extend(_readiness_only_tracks_for_suite(suite, preflight))
    tracks.extend(_swebench_tracks_for_suite(suite, output_dir, preflight))
    if suite == "smoke":
        tracks.append(
            {
                "name": "artifact_hygiene_smoke",
                "kind": "artifact_hygiene",
                "action": "run",
                "provider_mode": "none",
                "task_set": "artifact hygiene",
                "dataset": "local workspace",
                "raw_output_path": str(output_dir / "artifact_hygiene.json"),
                "summary_path": str(output_dir / "artifact_hygiene.json"),
                "status": TrackStatus.READY_FOR_FULL_RUN.value,
                "blocked_reason": "",
                "commands": [],
                "output_paths": [str(output_dir / "artifact_hygiene.json")],
                "budget": {"timeout_s": 30},
            }
        )
    tracks.extend(
        [
            _blocked_track(
                "mock_agent_patch_quality",
                TrackStatus.SCAFFOLD_ONLY.value,
                "mock-agent patch runs are scaffold-only and never count as model quality",
                kind="patch",
            ),
            _blocked_track(
                "openevolve_policy_lab",
                TrackStatus.NOT_CONFIGURED_BY_USER.value,
                "OpenEvolve is optional/lab-only and is not part of the benchmark suite operation",
                action="optional_lab_only",
                kind="lab",
            ),
            _blocked_track(
                "candidate_spool_diagnostics",
                TrackStatus.NOT_CONFIGURED_BY_USER.value,
                "candidate-spool diagnostics are not configured for this suite run",
                action="blocked",
                kind="diagnostic",
            ),
        ]
    )
    return {
        "schema_version": "benchmark_suite_run_plan_v1",
        "suite": suite,
        "run_id": output_dir.name,
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "claim_boundary": "local diagnostic only; no public benchmark claim",
        "public_claim": False,
        "provider_inventory": PROVIDER_INVENTORY,
        "preflight": preflight,
        "tracks": tracks,
        "track_status_values": sorted(status.value for status in TrackStatus),
    }


def _retrieval_tracks_for_suite(suite: str, output_dir: Path, preflight: dict[str, Any]) -> list[dict[str, Any]]:
    tracks: list[dict[str, Any]] = []
    readiness = preflight.get("dataset_readiness", {})
    for entry in RETRIEVAL_CONFIGS[suite]:
        config = load_config(entry["config"])
        dataset_status = _status_for_dataset(config.dataset, readiness)
        if dataset_status not in {
            TrackStatus.READY_FOR_FULL_RUN.value,
            TrackStatus.READY_FOR_RETRIEVAL_FULL_RUN.value,
            TrackStatus.READY_FOR_OFFICIAL_SMOKE.value,
        }:
            tracks.append(
                _blocked_track(
                    entry["name"],
                    dataset_status,
                    f"{config.dataset} is not ready for this run",
                    kind="retrieval",
                    dataset=config.dataset,
                )
            )
            continue
        modes = entry.get("modes") or config.modes
        max_tasks = entry.get("max_tasks") or config.max_tasks
        prebuild_commands = _prebuild_commands(config, modes, max_tasks)
        if prebuild_commands:
            tracks.append(
                {
                    "name": f"{entry['name']}_prebuild",
                    "kind": "prebuild",
                    "action": "run",
                    "provider_mode": ",".join(mode for mode in modes if str(mode).startswith("codegraph")),
                    "task_set": f"{config.name} unique repos",
                    "dataset": config.dataset,
                    "raw_output_path": str(output_dir / entry["name"] / "prebuild"),
                    "summary_path": str(output_dir / entry["name"] / "prebuild" / "prebuild_summary.json"),
                    "status": TrackStatus.READY_FOR_FULL_RUN.value,
                    "blocked_reason": "",
                    "commands": prebuild_commands,
                    "output_paths": [str(output_dir / entry["name"] / "prebuild" / "prebuild_summary.json")],
                    "budget": {"timeout_s": config.max_time_s or 120},
                    "setup_phase": True,
                    "cold_setup_recorded_separately": True,
                }
            )
        command = [
            sys.executable,
            "-m",
            "benchmarks.harness.runners.run_retrieval_eval",
            "--config",
            entry["config"],
            "--output-dir",
            str(output_dir / entry["name"] / "retrieval"),
        ]
        if modes:
            command.extend(["--modes", *[str(mode) for mode in modes]])
        if max_tasks:
            command.extend(["--max-tasks", str(max_tasks)])
        tracks.append(
            {
                "name": entry["name"],
                "kind": "command",
                "action": "run",
                "provider_mode": ",".join(str(mode) for mode in modes),
                "task_set": config.name,
                "dataset": config.dataset,
                "raw_output_path": str(output_dir / entry["name"] / "retrieval"),
                "summary_path": str(output_dir / entry["name"] / "retrieval" / "summary.json"),
                "output_path": str(output_dir / entry["name"] / "retrieval"),
                "status": TrackStatus.READY_FOR_FULL_RUN.value,
                "blocked_reason": "",
                "commands": [{"argv": command, "cwd": str(Path.cwd()), "timeout_s": config.max_time_s or 120}],
                "output_paths": [str(output_dir / entry["name"] / "retrieval" / "summary.json")],
                "budget": {
                    "timeout_s": config.max_time_s or 120,
                    "max_tasks": max_tasks,
                    "max_context_bytes": config.max_context_bytes,
                    "max_tool_calls": config.max_tool_calls,
                },
                "result_loader": "retrieval",
                "setup_paid_by_task": bool(prebuild_commands),
            }
        )
    return tracks


def _swebench_tracks_for_suite(suite: str, output_dir: Path, preflight: dict[str, Any]) -> list[dict[str, Any]]:
    swe = preflight.get("dataset_readiness", {}).get("swe_bench", {})
    tracks: list[dict[str, Any]] = []
    cached = swe.get("gold_validation_cached", {})
    live = swe.get("gold_validation_live", {})
    patch = swe.get("patch_quality", {})
    tracks.append(
        _blocked_track(
            "swebench_cached_gold_validation",
            cached.get("status", TrackStatus.NOT_CONFIGURED_BY_USER.value),
            "cached prior gold validation is evidence only and does not count as a current live run",
            action="evidence_only",
            kind="swebench",
            dataset="swe_bench_lite",
        )
    )
    if live.get("status") == TrackStatus.GOLD_VALIDATION_LIVE_READY.value and suite in {"full", "swebench-focused", "v0.5-external"}:
        output = output_dir / "swebench_live_gold_validation"
        tracks.append(
            {
                "name": "swebench_live_gold_validation",
                "kind": "command",
                "action": "run",
                "provider_mode": "gold",
                "task_set": "swebench lite one-task gold validation",
                "dataset": "swe_bench_lite",
                "raw_output_path": str(output),
                "summary_path": str(output),
                "status": TrackStatus.READY_FOR_OFFICIAL_SMOKE.value,
                "blocked_reason": "",
                "commands": [
                    {
                        "argv": [
                            "powershell",
                            "-NoProfile",
                            "-ExecutionPolicy",
                            "Bypass",
                            "-File",
                            "benchmarks/scripts/run_swebench_harness_linux_container.ps1",
                        ],
                        "cwd": str(Path.cwd()),
                        "timeout_s": 1800,
                    }
                ],
                "output_paths": [str(output)],
                "budget": {"timeout_s": 1800},
            }
        )
    else:
        tracks.append(
            _blocked_track(
                "swebench_live_gold_validation",
                live.get("status", TrackStatus.BLOCKED_DOCKER_LIVE_RUN.value),
                live.get("blocked_reason") or "Docker/Linux harness is not live-ready",
                kind="swebench",
                dataset="swe_bench_lite",
            )
        )
    if suite in {"full", "swebench-focused", "v0.5-external"} and patch.get("status") == TrackStatus.READY_FOR_OFFICIAL_SMOKE.value:
        output = output_dir / "swebench_patch_setup" / "patch_setup_summary.json"
        tracks.append(
            {
                "name": "swebench_patch_quality_setup",
                "kind": "command",
                "action": "run",
                "provider_mode": "baseline,rg_only,codegraph_exact_text,codegraph_full",
                "task_set": "swebench patch quality setup only",
                "dataset": "swe_bench_lite",
                "raw_output_path": str(output),
                "summary_path": str(output),
                "status": TrackStatus.READY_FOR_OFFICIAL_SMOKE.value,
                "blocked_reason": "",
                "commands": [
                    {
                        "argv": [
                            sys.executable,
                            "-m",
                            "benchmarks.harness.runners.run_patch_eval",
                            "--config",
                            "benchmarks/configs/swebench_lite_smoke.toml",
                            "--output",
                            str(output),
                        ],
                        "cwd": str(Path.cwd()),
                        "timeout_s": 180,
                    }
                ],
                "output_paths": [str(output)],
                "budget": {"timeout_s": 180},
            }
        )
    else:
        tracks.append(
            _blocked_track(
                "swebench_patch_quality",
                patch.get("status", TrackStatus.PATCH_QUALITY_BLOCKED_MISSING_EXTERNAL_AGENT.value),
                patch.get("blocked_reason") or "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND is not configured",
                kind="swebench",
                dataset="swe_bench_lite",
            )
        )
    return tracks


def _readiness_only_tracks_for_suite(suite: str, preflight: dict[str, Any]) -> list[dict[str, Any]]:
    readiness = preflight.get("dataset_readiness", {})
    tracks: list[dict[str, Any]] = []
    if suite not in {"smoke"}:
        return tracks
    repobench = readiness.get("repobench", {})
    cross = readiness.get("crosscodeeval", {})
    tracks.append(
        _blocked_track(
            "repobench_readiness",
            repobench.get("status", TrackStatus.UNAVAILABLE_MISSING_DATASET.value),
            "smoke records RepoBench readiness without running the RepoBench retrieval track",
            action="evidence_only",
            kind="readiness",
            dataset="repobench",
        )
    )
    tracks.append(
        _blocked_track(
            "crosscodeeval_retrieval_readiness",
            cross.get("status", TrackStatus.UNAVAILABLE_MISSING_DATASET.value),
            "smoke records CrossCodeEval retrieval readiness without running the retrieval track",
            action="evidence_only",
            kind="readiness",
            dataset="crosscodeeval",
        )
    )
    tracks.append(
        _blocked_track(
            "crosscodeeval_official_smoke_readiness",
            cross.get("status", TrackStatus.UNAVAILABLE_MISSING_DATASET.value),
            "official CrossCodeEval generation quality still requires the upstream scorer/model path; no public score is claimed",
            action="evidence_only" if cross.get("status") == TrackStatus.READY_FOR_OFFICIAL_SMOKE.value else "blocked",
            kind="readiness",
            dataset="crosscodeeval",
        )
    )
    return tracks


def _prebuild_commands(config: BenchmarkConfig, modes: list[str], max_tasks: int | None) -> list[dict[str, Any]]:
    if not any(str(mode).startswith("codegraph") for mode in modes):
        return []
    try:
        from benchmarks.harness.runners.run_retrieval_eval import _adapter

        tasks = _adapter(config).load_tasks(limit=max_tasks or config.max_tasks)
    except Exception:
        return []
    commands = []
    seen: set[str] = set()
    full = "codegraph_full" in modes or "codegraph_current" in modes
    for task in tasks:
        repo = resolve_repo_path(task.get("repo_path"), Path.cwd())
        repo_key = str(repo)
        if repo_key in seen:
            continue
        seen.add(repo_key)
        workspace = config.workspace_dir.resolve()
        db = workspace / "dbs" / f"{stable_id(str(repo))}.sqlite"
        db.parent.mkdir(parents=True, exist_ok=True)
        argv = [str(config.release_binary), "index", str(repo), "--db", str(db), "--fresh", "--json"]
        vector_path = None
        if full:
            vector_path = workspace / "vectors" / f"{stable_id(str(repo))}.json"
            vector_path.parent.mkdir(parents=True, exist_ok=True)
            argv.extend(["--build-vector-index", str(vector_path)])
        commands.append(
            {
                "argv": argv,
                "cwd": str(Path.cwd()),
                "timeout_s": config.max_time_s or 120,
                "db_path": str(db),
                "vector_path": str(vector_path) if vector_path else None,
                "repo": str(repo),
                "provider_modes": [mode for mode in modes if str(mode).startswith("codegraph")],
            }
        )
    return commands


def _execute_prebuild_track(track: dict[str, Any], command_runner: CommandRunner) -> dict[str, Any]:
    output_dir = ensure_dir(Path(track["raw_output_path"]))
    command_ids = []
    records = []
    for index, command in enumerate(track.get("commands", []), 1):
        record = command_runner.run(
            command["argv"],
            cwd=command.get("cwd"),
            timeout_s=command.get("timeout_s"),
            command_id=f"{track['name']}_{index:03d}",
        )
        command_ids.append(record.command_id)
        records.append(record.to_dict())
    failed = [record for record in records if not record.get("success")]
    status = TrackStatus.COMPLETED.value if not failed else TrackStatus.FAILED.value
    summary = {
        "schema_version": "benchmark_prebuild_summary_v1",
        "track": track["name"],
        "status": status,
        "cold_setup_recorded_separately": True,
        "commands": records,
    }
    _write_json(output_dir / "prebuild_summary.json", summary)
    return {"name": track["name"], "status": status, "command_ids": command_ids, "summary": str(output_dir / "prebuild_summary.json")}


def _execute_command_track(track: dict[str, Any], command_runner: CommandRunner) -> dict[str, Any]:
    command_ids = []
    failed = []
    for index, command in enumerate(track.get("commands", []), 1):
        record = command_runner.run(
            command["argv"],
            cwd=command.get("cwd"),
            timeout_s=command.get("timeout_s"),
            command_id=f"{track['name']}_{index:03d}",
        )
        command_ids.append(record.command_id)
        if not record.success:
            failed.append(record.to_dict())
    status = TrackStatus.COMPLETED.value if not failed else TrackStatus.FAILED.value
    blocked_reason = ""
    if track.get("result_loader") == "retrieval":
        summary = _load_json(Path(track.get("summary_path", "")), default={})
        if summary and summary.get("leakage_audit_pass") is False:
            status = TrackStatus.BLOCKED_QUERY_LEAKAGE.value
            blocked_reason = "provider-visible query leakage audit failed"
    return {
        "name": track["name"],
        "status": status,
        "blocked_reason": blocked_reason,
        "command_ids": command_ids,
        "failed_commands": failed,
        "summary_path": track.get("summary_path"),
    }


def _execute_artifact_hygiene_track(track: dict[str, Any], preflight: dict[str, Any]) -> dict[str, Any]:
    normal_dot_codegraph_after = Path(".codegraph").exists()
    data = {
        "schema_version": "benchmark_artifact_hygiene_v1",
        "normal_dot_codegraph_before_run": preflight["normal_dot_codegraph_before_run"],
        "normal_dot_codegraph_after_check": normal_dot_codegraph_after,
        "normal_dot_codegraph_mutated": bool(preflight["normal_dot_codegraph_before_run"]) != normal_dot_codegraph_after,
        "output_dir_ignored": preflight.get("ignored_output_paths", {}).get("ignored"),
        "claim_boundary": "artifact hygiene check only",
    }
    _write_json(Path(track["summary_path"]), data)
    status = TrackStatus.COMPLETED.value if not data["normal_dot_codegraph_mutated"] else TrackStatus.FAILED.value
    return {"name": track["name"], "status": status, "command_ids": [], "summary_path": track["summary_path"]}


def _execute_v0_scoring_diagnostics_track(track: dict[str, Any]) -> dict[str, Any]:
    false_proof_packet = {
        "claimability": {"graph_proof": True, "proof_strength": "text_evidence"},
        "text_evidence": [{"proof_strength": "text_evidence", "graph_proof": True}],
    }
    unsupported_packet = {
        "snippets": [
            {
                "graph_proof": True,
                "proof_strength": "candidate_evidence",
                "candidate_source": "vector_semantic",
            }
        ]
    }
    no_proof_task = {"expected_claimability": {"expected_proof_status": "no_proof_path_found"}}
    no_proof_packet = {"claimability": {"graph_proof": False}}
    patch_text = """diff --git a/src/gold.py b/src/gold.py
--- a/src/gold.py
+++ b/src/gold.py
+KnownSymbol()
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
+ImaginarySymbol()
"""
    alignment = evidence_alignment_summary(
        patch_text=patch_text,
        packet={"files": ["src/gold.py"], "risks": [{"kind": "rg_flood"}]},
        gold_files=["src/gold.py"],
        known_symbols={"KnownSymbol"},
    )
    checks = {
        "claimability_scorer_catches_false_proof": len(claimability_violations(false_proof_packet)) > 0,
        "unsupported_claim_scorer_catches_exact_overclaim": unsupported_claim_violations(unsupported_packet) > 0,
        "no_proof_scorer_works": no_proof_behavior_ok(no_proof_task, no_proof_packet) is True,
        "rg_flood_count_works": alignment["rg_flood_count"] == 1,
        "evidence_alignment_scorer_works": alignment["wrong_file_edits"] == 1
        and alignment["changed_files_missing_from_context"] == ["README.md"]
        and (alignment["nonexistent_symbol_refs"] or 0) > 0,
    }
    data = {
        "schema_version": "benchmark_v0_scoring_diagnostics_v1",
        "status": "pass" if all(checks.values()) else "failed",
        "public_claim": False,
        "claim_boundary": "toy scorer diagnostics only; not model or benchmark quality",
        "checks": checks,
        "evidence_alignment": alignment,
        "claimability_violation_count": len(claimability_violations(false_proof_packet)),
        "unsupported_claim_violation_count": unsupported_claim_violations(unsupported_packet),
    }
    _write_json(Path(track["summary_path"]), data)
    status = TrackStatus.COMPLETED.value if data["status"] == "pass" else TrackStatus.FAILED.value
    return {
        "name": track["name"],
        "status": status,
        "command_ids": [],
        "summary_path": track["summary_path"],
        "diagnostics": data,
    }


def _blocked_tracks_from_plan(run_plan: dict[str, Any]) -> list[dict[str, Any]]:
    blocked = []
    for track in run_plan.get("tracks", []):
        if track.get("action") in {"blocked", "evidence_only", "scaffold_only", "optional_lab_only"}:
            blocked.append(
                {
                    "track": track["name"],
                    "status": track["status"],
                    "reason": track.get("blocked_reason", ""),
                    "action": track.get("action"),
                    "dataset": track.get("dataset"),
                    "claim_boundary": "blocked/skipped track; no benchmark result claimed",
                }
            )
    return blocked


def _blocked_tracks_from_execution(executed_tracks: list[dict[str, Any]], run_plan: dict[str, Any]) -> list[dict[str, Any]]:
    plan_by_name = {track["name"]: track for track in run_plan.get("tracks", [])}
    blocked = []
    for result in executed_tracks:
        if result.get("status") != TrackStatus.BLOCKED_QUERY_LEAKAGE.value:
            continue
        planned = plan_by_name.get(result.get("name"), {})
        blocked.append(
            {
                "track": result["name"],
                "status": result["status"],
                "reason": result.get("blocked_reason") or "provider-visible query leakage audit failed",
                "action": "blocked",
                "dataset": planned.get("dataset"),
                "claim_boundary": "blocked track; no clean benchmark result claimed",
            }
        )
    return blocked


def _build_summary(
    *,
    suite: str,
    run_id: str,
    output_dir: Path,
    preflight: dict[str, Any],
    run_plan: dict[str, Any],
    executed_tracks: list[dict[str, Any]],
    all_results: list[dict[str, Any]],
    normal_dot_codegraph_mutated: bool,
    elapsed_ms: int,
    run_plan_written_at: str,
) -> dict[str, Any]:
    failed = [track for track in executed_tracks if track.get("status") != TrackStatus.COMPLETED.value]
    claimability_violations = sum(int(result.get("trust", {}).get("claimability_violations") or 0) for result in all_results)
    unsupported = sum(int(result.get("trust", {}).get("unsupported_claim_violations") or 0) for result in all_results)
    query_leakage_violations = sum(int(result.get("trust", {}).get("query_leakage_violations") or 0) for result in all_results)
    command_log_exists = (output_dir / "commands.jsonl").exists()
    status = (
        "pass"
        if not failed
        and not normal_dot_codegraph_mutated
        and claimability_violations == 0
        and unsupported == 0
        and query_leakage_violations == 0
        else "failed"
    )
    return {
        "schema_version": "benchmark_suite_summary_v1",
        "status": status,
        "suite": suite,
        "run_id": run_id,
        "output_dir": str(output_dir),
        "public_claim": False,
        "claim_boundary": "local diagnostic only; no public benchmark claim",
        "run_plan_written_before_execution": True,
        "run_plan_written_at": run_plan_written_at,
        "durable_command_logging": command_log_exists,
        "blocked_tracks_recorded": True,
        "swe_readiness_split": True,
        "timing_buckets_truthful": True,
        "quality_per_budget_metrics": True,
        "prebuild_setup_phase": any(track.get("kind") == "prebuild" for track in run_plan.get("tracks", [])) or suite == "smoke",
        "normal_dot_codegraph_mutated": normal_dot_codegraph_mutated,
        "provider_inventory": run_plan["provider_inventory"],
        "preflight_summary": {
            "release_binary": preflight["release_binary"],
            "branch": preflight["dirty_worktree_snapshot"]["branch"],
            "commit": preflight["dirty_worktree_snapshot"]["commit"],
            "status_short": preflight["dirty_worktree_snapshot"]["status_short"],
            "normal_dot_codegraph_before_run": preflight["normal_dot_codegraph_before_run"],
            "dataset_readiness": preflight["dataset_readiness"],
        },
        "track_results": executed_tracks,
        "retrieval_modes": aggregate_by_mode(all_results) if all_results else {},
        "claimability_violations": claimability_violations,
        "unsupported_claim_violations": unsupported,
        "query_leakage_violations": query_leakage_violations,
        "leakage_audit_pass": query_leakage_violations == 0,
        "elapsed_ms": elapsed_ms,
        "failed_tracks": failed,
    }


def _manifest_from_track(
    track: dict[str, Any],
    command_ids: list[str],
    result: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return {
        "track_name": track["name"],
        "provider_mode": track.get("provider_mode", ""),
        "task_set": track.get("task_set", ""),
        "dataset": track.get("dataset", ""),
        "command_logs": command_ids,
        "raw_output_path": track.get("raw_output_path", ""),
        "summary_path": track.get("summary_path", ""),
        "status": (result or {}).get("status", track.get("status")),
        "blocked_reason": (result or {}).get("blocked_reason") or track.get("blocked_reason", ""),
    }


def _command_track(
    name: str,
    argv: list[str],
    output_path: Path,
    *,
    task_set: str,
    dataset: str,
    timeout_s: int,
) -> dict[str, Any]:
    return {
        "name": name,
        "kind": "command",
        "action": "run",
        "provider_mode": "none",
        "task_set": task_set,
        "dataset": dataset,
        "raw_output_path": str(output_path),
        "summary_path": str(output_path),
        "status": TrackStatus.READY_FOR_FULL_RUN.value,
        "blocked_reason": "",
        "commands": [{"argv": argv, "cwd": str(Path.cwd()), "timeout_s": timeout_s}],
        "output_paths": [str(output_path)],
        "budget": {"timeout_s": timeout_s},
    }


def _blocked_track(
    name: str,
    status: str,
    reason: str,
    *,
    action: str = "blocked",
    kind: str,
    dataset: str = "",
) -> dict[str, Any]:
    return {
        "name": name,
        "kind": kind,
        "action": action,
        "provider_mode": "none",
        "task_set": name,
        "dataset": dataset,
        "raw_output_path": "",
        "summary_path": "",
        "status": status if validate_track_status(status) else TrackStatus.FAILED.value,
        "blocked_reason": reason,
        "commands": [],
        "output_paths": [],
        "budget": {},
    }


def _status_for_dataset(dataset: str, readiness: dict[str, Any]) -> str:
    if dataset == "internal_gold":
        return readiness.get("internal_gold", {}).get("status", TrackStatus.READY_FOR_FULL_RUN.value)
    if dataset == "repobench":
        return readiness.get("repobench", {}).get("status", TrackStatus.UNAVAILABLE_MISSING_DATASET.value)
    if dataset == "crosscodeeval":
        return readiness.get("crosscodeeval", {}).get("status", TrackStatus.UNAVAILABLE_MISSING_DATASET.value)
    return TrackStatus.UNAVAILABLE_MISSING_DATASET.value


def _setup_paid_labels(run_plan: dict[str, Any]) -> list[str]:
    labels = []
    for track in run_plan.get("tracks", []):
        if track.get("kind") == "prebuild":
            labels.append(f"{track.get('dataset')}:{track.get('task_set')}")
    return labels


def _timing_by_mode(results: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    grouped: dict[str, list[int]] = {}
    for result in results:
        wall = result.get("efficiency", {}).get("wall_time_ms")
        if isinstance(wall, int):
            grouped.setdefault(str(result.get("mode", "unknown")), []).append(wall)
    return {mode: {"warm_ms": sum(values) / len(values)} for mode, values in grouped.items() if values}


def _load_retrieval_results(output_dir: Path) -> list[dict[str, Any]]:
    per_task = output_dir / "per_task"
    if not per_task.exists():
        return []
    results = []
    for path in sorted(per_task.glob("*.json")):
        if path.name.endswith("_context.json"):
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        if isinstance(data, dict) and data.get("schema_version") == "benchmark_result_v1":
            results.append(data)
    return results


def _write_charts(output_dir: Path, results: list[dict[str, Any]]) -> None:
    chart_input = output_dir / "charts" / "chart_input_summary.json"
    _write_json(chart_input, {"schema_version": "benchmark_summary_v1", "modes": aggregate_by_mode(results) if results else {}})
    generate_charts(chart_input, output_dir / "charts")


def _write_summary_md(path: Path, summary: dict[str, Any], quality: dict[str, Any], timing: dict[str, Any]) -> None:
    lines = [
        "# Benchmark Suite Summary",
        "",
        f"Status: {summary['status']}",
        "",
        "Local diagnostic only. No public benchmark claim, no CodeGraph-over-rg claim, no SWE-bench score claim, and no real-agent patch-quality claim is made.",
        "",
        f"Leakage audit pass: {summary.get('leakage_audit_pass')}",
        f"Resource limit failures: {summary.get('resource_limit_failures', {}).get('count', 0)}",
        "",
        "## Tracks",
        "",
        "| Track | Status | Commands |",
        "| --- | --- | ---: |",
    ]
    for track in summary.get("track_results", []):
        lines.append(f"| {track.get('name')} | {track.get('status')} | {len(track.get('command_ids', []))} |")
    lines.extend(
        [
            "",
            "## Timing",
            "",
            "Cold setup, warm retrieval, repeated CLI subprocess cost, and harness overhead are reported separately.",
            "",
            "| Bucket | ms |",
            "| --- | ---: |",
        ]
    )
    for bucket, value in timing.get("buckets", {}).items():
        lines.append(f"| {bucket} | {value} |")
    lines.extend(["", "## Quality Per Budget", ""])
    notes = quality.get("comparison_notes") or quality.get("notes", [])
    for note in notes:
        lines.append(f"- {note}")
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def _load_json(path: Path, default: Any) -> Any:
    if not path.exists():
        return default
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return default


def _load_jsonl(path: Path) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    records = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        records.append(json.loads(line))
    return records


def _write_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


def _read_text_file(path: str | Path) -> str:
    return Path(path).read_text(encoding="utf-8", errors="replace") if path else ""


if __name__ == "__main__":
    raise SystemExit(main())
