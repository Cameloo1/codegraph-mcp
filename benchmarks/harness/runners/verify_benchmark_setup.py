from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

from benchmarks.harness.adapters.crosscodeeval_adapter import CrossCodeEvalAdapter
from benchmarks.harness.adapters.internal_gold_adapter import InternalGoldAdapter
from benchmarks.harness.adapters.repobench_adapter import RepoBenchAdapter
from benchmarks.harness.adapters.swebench_adapter import SWEBenchAdapter
from benchmarks.harness.config import load_config, validate_config
from benchmarks.harness.context_providers.codegraph_exact_text import CodeGraphExactTextProvider
from benchmarks.harness.context_providers.codegraph_full import CodeGraphFullProvider
from benchmarks.harness.context_providers.none import NoneProvider
from benchmarks.harness.context_providers.rg_only import RgOnlyProvider
from benchmarks.harness.paths import dataset_path, upstream_path, workspace_path
from benchmarks.harness.runners.run_patch_eval import patch_setup_summary


CONFIGS = [
    "benchmarks/tracks/internal_gold/configs/smoke.toml",
    "benchmarks/tracks/internal_gold/configs/full.toml",
    "benchmarks/tracks/repobench/configs/smoke.toml",
    "benchmarks/tracks/repobench/configs/small.toml",
    "benchmarks/tracks/crosscodeeval/configs/smoke.toml",
    "benchmarks/tracks/crosscodeeval/configs/small.toml",
    "benchmarks/tracks/swebench_lite/configs/smoke.toml",
    "benchmarks/tracks/swebench_lite/configs/lite_10.toml",
    "benchmarks/tracks/swebench_lite/configs/patch_runner_external_agent.example.toml",
    "benchmarks/tracks/swebench_lite/configs/codex_external_agent.example.toml",
    "benchmarks/configs/internal_gold_smoke.toml",
    "benchmarks/configs/repobench_smoke.toml",
    "benchmarks/configs/crosscodeeval_smoke.toml",
    "benchmarks/configs/swebench_lite_smoke.toml",
]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args(argv)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    summary = verify_setup()
    (output_dir / "setup_summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    (output_dir / "setup_summary.md").write_text(_markdown(summary), encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 0 if summary["status"] in {"ready_for_dedicated_internal_run", "ready_with_external_blockers"} else 1


def verify_setup() -> dict[str, Any]:
    internal_adapter = InternalGoldAdapter(dataset_path("internal_gold", "internal_gold"))
    internal_tasks = internal_adapter.load_tasks()
    repobench = RepoBenchAdapter(workspace_path("repobench", "repobench_data"))
    crosscodeeval = CrossCodeEvalAdapter(upstream_path("crosscodeeval", "cceval", "data"))
    swebench = SWEBenchAdapter(upstream_path("swebench_lite", "SWE-bench"))
    adapter_status = {
        "internal_gold": {
            "setup_state": "ready",
            "status": "ready",
            "ready": True,
            "smoke_ready": True,
            "task_count": len(internal_tasks),
        },
        "repobench": repobench.setup_status().to_dict(),
        "crosscodeeval": crosscodeeval.setup_status().to_dict(),
        "swe_bench_lite": swebench.setup_status().to_dict(),
    }
    smoke_loads = {
        "internal_gold_tasks": len(internal_tasks),
        "repobench_real_tasks": _count_or_blocker(lambda: repobench.load_tasks(limit=5)),
        "repobench_fixture_tasks": _count_or_blocker(lambda: repobench.load_fixture_tasks(limit=5)),
        "crosscodeeval_real_tasks": _count_or_blocker(lambda: crosscodeeval.load_tasks(limit=5)),
        "crosscodeeval_fixture_tasks": _count_or_blocker(lambda: crosscodeeval.load_fixture_tasks(limit=5)),
    }
    config_status = _config_status()
    provider_isolation = _provider_isolation()
    ignored_paths = _ignored_path_status()
    pinned_sources = _load_json(Path("benchmarks/upstream/pinned_sources.json"))
    pinned_sources_status = _pinned_sources_status(pinned_sources)
    patch_status = patch_setup_summary("benchmarks/tracks/swebench_lite/configs/smoke.toml")
    docs = {
        "benchmarks_readme": Path("benchmarks/README.md").exists(),
        "benchmark_claims": Path("benchmarks/BENCHMARK_CLAIMS.md").exists(),
        "benchmarks_plan": Path("benchmarks/BENCHMARKS.md").exists(),
    }
    tracked_artifacts = _tracked_artifacts()
    external_ready = all(
        bool(status.get("ready"))
        for name, status in adapter_status.items()
        if name in {"repobench", "crosscodeeval", "swe_bench_lite"}
    )
    status = "ready_for_dedicated_internal_run"
    if not external_ready or patch_status["external_agent_command"]["status"] != "ready":
        status = "ready_with_external_blockers"
    return {
        "schema_version": "benchmark_setup_verification_v1",
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "status": status,
        "claim_boundary": "setup verification only; not a benchmark score or public result",
        "adapters": adapter_status,
        "smoke_loads": smoke_loads,
        "configs": config_status,
        "provider_isolation": provider_isolation,
        "ignored_paths": ignored_paths,
        "pinned_sources": pinned_sources,
        "pinned_sources_status": pinned_sources_status,
        "patch_runner": patch_status,
        "docs": docs,
        "tracked_large_artifacts": tracked_artifacts,
        "full_benchmark_run_allowed": {
            "internal_gold": True,
            "repobench": bool(adapter_status["repobench"].get("ready")),
            "crosscodeeval": bool(adapter_status["crosscodeeval"].get("ready")),
            "swe_bench_lite": bool(adapter_status["swe_bench_lite"].get("ready"))
            and patch_status["external_agent_command"]["status"] == "ready",
        },
        "track_readiness": {
            "internal_gold": "ready_for_full_run",
            "repobench": adapter_status["repobench"].get("status"),
            "crosscodeeval": adapter_status["crosscodeeval"].get("status"),
            "swe_bench_lite": (
                "ready_for_patch_scoring"
                if bool(adapter_status["swe_bench_lite"].get("ready"))
                and patch_status["external_agent_command"]["status"] == "ready"
                else "hard_external_prerequisite_missing_external_agent_command"
                if bool(adapter_status["swe_bench_lite"].get("ready"))
                else adapter_status["swe_bench_lite"].get("status")
            ),
        },
}


def _count_or_blocker(loader) -> dict[str, Any]:
    try:
        tasks = loader()
        return {"status": "ready", "count": len(tasks)}
    except Exception as exc:  # setup probes must report blockers, not explode
        return {"status": "blocked", "blocker": str(exc)}


def _config_status() -> dict[str, Any]:
    entries = {}
    for config_path in CONFIGS:
        path = Path(config_path)
        if not path.exists():
            entries[config_path] = {"status": "missing", "errors": ["config missing"]}
            continue
        try:
            config = load_config(path)
            errors = validate_config(config)
        except Exception as exc:
            errors = [str(exc)]
        entries[config_path] = {"status": "ready" if not errors else "invalid", "errors": errors}
    return entries


def _provider_isolation() -> dict[str, Any]:
    repo = Path.cwd()
    workspace = workspace_path("internal_gold", "setup-verification-provider-isolation")
    release = Path("target/release/codegraph-mcp.exe")
    providers = [
        NoneProvider(repo, workspace, release),
        RgOnlyProvider(repo, workspace, release),
        CodeGraphExactTextProvider(repo, workspace, release),
        CodeGraphFullProvider(repo, workspace, release),
    ]
    metadata = {provider.mode: provider.metadata() for provider in providers}
    errors = []
    if metadata["rg_only"]["uses_codegraph"]:
        errors.append("rg_only reports CodeGraph usage")
    if metadata["codegraph_exact_text"]["uses_rg"] or metadata["codegraph_full"]["uses_rg"]:
        errors.append("CodeGraph provider reports rg usage")
    if metadata["codegraph_exact_text"].get("vector_candidates_enabled"):
        errors.append("codegraph_exact_text enables vector candidates")
    if not metadata["codegraph_full"].get("vector_candidates_enabled"):
        errors.append("codegraph_full does not report vector candidates")
    return {"status": "ready" if not errors else "invalid", "metadata": metadata, "errors": errors}


def _ignored_path_status() -> dict[str, Any]:
    paths = [
        "benchmarks/results/summaries/setup_verification_probe",
        "benchmarks/workspaces/setup_verification_probe",
        "benchmarks/upstream/SWE-bench",
        "benchmarks/tracks/internal_gold/results/setup_verification_probe",
        "benchmarks/tracks/repobench/workspaces/setup_verification_probe",
        "benchmarks/tracks/swebench_lite/upstream/SWE-bench",
        "reports/audit/artifacts/benchmark_external_setup_full/logs/probe.log",
    ]
    result = {}
    for path in paths:
        proc = subprocess.run(["git", "check-ignore", path], text=True, capture_output=True, check=False)
        result[path] = {"ignored": proc.returncode == 0, "stdout": proc.stdout.strip(), "stderr": proc.stderr.strip()}
    return result


def _tracked_artifacts() -> dict[str, Any]:
    proc = subprocess.run(
        [
            "git",
            "ls-files",
            "benchmarks/results",
            "benchmarks/workspaces",
            "benchmarks/upstream/SWE-bench",
            "benchmarks/upstream/repobench",
            "benchmarks/upstream/cceval",
            "benchmarks/tracks/internal_gold/results",
            "benchmarks/tracks/internal_gold/workspaces",
            "benchmarks/tracks/repobench/results",
            "benchmarks/tracks/repobench/workspaces",
            "benchmarks/tracks/repobench/upstream",
            "benchmarks/tracks/crosscodeeval/results",
            "benchmarks/tracks/crosscodeeval/workspaces",
            "benchmarks/tracks/crosscodeeval/upstream",
            "benchmarks/tracks/swebench_lite/results",
            "benchmarks/tracks/swebench_lite/workspaces",
            "benchmarks/tracks/swebench_lite/upstream",
        ],
        text=True,
        capture_output=True,
        check=False,
    )
    files = [line for line in proc.stdout.splitlines() if line.strip() and not _allowed_tracked_artifact_marker(line.strip())]
    return {"status": "clean" if not files else "tracked_artifacts_present", "files": files, "stderr": proc.stderr.strip()}


def _allowed_tracked_artifact_marker(path: str) -> bool:
    return path.endswith(("/README.md", "/.gitkeep", "/pinned_source.json", "/pinned_sources.json"))


def _load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"status": "missing"}
    return json.loads(path.read_text(encoding="utf-8-sig"))


def _pinned_sources_status(data: dict[str, Any]) -> dict[str, Any]:
    sources = data.get("sources", []) if isinstance(data, dict) else []
    required = {
        "source_name",
        "official_url",
        "local_path",
        "pinned_commit_or_version",
        "setup_status",
        "clone_command",
        "install_command",
        "dataset_command",
        "smoke_command",
        "scoring_command",
        "exact_remaining_action",
    }
    missing_entries = []
    for index, source in enumerate(sources):
        if not isinstance(source, dict):
            missing_entries.append({"index": index, "missing": ["source object"]})
            continue
        missing = sorted(field for field in required if field not in source)
        if missing:
            missing_entries.append({"index": index, "source": source.get("source_name") or source.get("name"), "missing": missing})
    return {
        "status": "ready" if sources and not missing_entries else "invalid",
        "source_count": len(sources),
        "missing_entries": missing_entries,
    }


def _markdown(summary: dict[str, Any]) -> str:
    lines = [
        "# Benchmark Setup Verification",
        "",
        f"Status: {summary['status']}",
        "",
        "This is setup verification only. It is not a benchmark score or public result.",
        "",
        "## Adapter Status",
        "",
        "| Adapter | State | Status | Ready | Smoke ready | Blockers |",
        "| --- | --- | --- | ---: | ---: | --- |",
    ]
    for name, status in summary["adapters"].items():
        blockers = "; ".join(status.get("blockers", [])) if isinstance(status, dict) else ""
        lines.append(
            f"| {name} | {status.get('setup_state')} | {status.get('status')} | {status.get('ready')} | {status.get('smoke_ready')} | {blockers} |"
        )
    lines.extend(["", "## Full Run Readiness", ""])
    for key, value in summary["full_benchmark_run_allowed"].items():
        lines.append(f"- {key}: {value}")
    lines.append("")
    return "\n".join(lines)


if __name__ == "__main__":
    raise SystemExit(main())
