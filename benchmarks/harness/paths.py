from __future__ import annotations

from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
BENCHMARKS_ROOT = REPO_ROOT / "benchmarks"
TRACKS_ROOT = BENCHMARKS_ROOT / "tracks"

TRACK_IDS = {
    "internal_gold",
    "repobench",
    "crosscodeeval",
    "swebench_lite",
    "graph_truth",
    "openevolve",
}


LEGACY_TO_CANONICAL = {
    "benchmarks/configs/internal_gold_smoke.toml": "benchmarks/tracks/internal_gold/configs/smoke.toml",
    "benchmarks/configs/internal_gold_full.toml": "benchmarks/tracks/internal_gold/configs/full.toml",
    "benchmarks/configs/repobench_smoke.toml": "benchmarks/tracks/repobench/configs/smoke.toml",
    "benchmarks/configs/repobench_small.toml": "benchmarks/tracks/repobench/configs/small.toml",
    "benchmarks/configs/crosscodeeval_smoke.toml": "benchmarks/tracks/crosscodeeval/configs/smoke.toml",
    "benchmarks/configs/crosscodeeval_small.toml": "benchmarks/tracks/crosscodeeval/configs/small.toml",
    "benchmarks/configs/swebench_lite_smoke.toml": "benchmarks/tracks/swebench_lite/configs/smoke.toml",
    "benchmarks/configs/swebench_lite_10.toml": "benchmarks/tracks/swebench_lite/configs/lite_10.toml",
    "benchmarks/configs/codex_external_agent.example.toml": "benchmarks/tracks/swebench_lite/configs/codex_external_agent.example.toml",
    "benchmarks/configs/patch_runner_external_agent.example.toml": "benchmarks/tracks/swebench_lite/configs/patch_runner_external_agent.example.toml",
    "benchmarks/datasets/internal_gold": "benchmarks/tracks/internal_gold/datasets/internal_gold",
    "benchmarks/datasets/adapter_fixtures/repobench_tiny.jsonl": "benchmarks/tracks/repobench/fixtures/repobench_tiny.jsonl",
    "benchmarks/datasets/adapter_fixtures/crosscodeeval_tiny.jsonl": "benchmarks/tracks/crosscodeeval/fixtures/crosscodeeval_tiny.jsonl",
    "benchmarks/datasets/adapter_fixtures/swebench_lite_sympy_20590.json": "benchmarks/tracks/swebench_lite/fixtures/swebench_lite_sympy_20590.json",
    "benchmarks/scripts/setup_repobench.ps1": "benchmarks/tracks/repobench/scripts/setup_repobench.ps1",
    "benchmarks/scripts/setup_crosscodeeval_docker.ps1": "benchmarks/tracks/crosscodeeval/scripts/setup_docker.ps1",
    "benchmarks/scripts/setup_crosscodeeval_windows_msvc.ps1": "benchmarks/tracks/crosscodeeval/scripts/setup_windows_msvc.ps1",
    "benchmarks/scripts/setup_crosscodeeval_wsl.ps1": "benchmarks/tracks/crosscodeeval/scripts/setup_wsl.ps1",
    "benchmarks/scripts/setup_crosscodeeval_wsl.sh": "benchmarks/tracks/crosscodeeval/scripts/setup_wsl.sh",
    "benchmarks/scripts/run_swebench_harness_linux_container.ps1": "benchmarks/tracks/swebench_lite/scripts/run_harness_linux_container.ps1",
    "benchmarks/scripts/run_codex_external_patch_agent.ps1": "benchmarks/tracks/swebench_lite/scripts/run_codex_external_patch_agent.ps1",
    "benchmarks/scripts/setup_swebench_wsl.ps1": "benchmarks/tracks/swebench_lite/scripts/setup_wsl.ps1",
    "benchmarks/scripts/setup_swebench_wsl.sh": "benchmarks/tracks/swebench_lite/scripts/setup_wsl.sh",
    "benchmarks/scripts/run_openevolve_candidate_spool_policy.ps1": "benchmarks/tracks/openevolve/scripts/run_candidate_spool_policy.ps1",
    "benchmarks/openevolve": "benchmarks/tracks/openevolve",
    "benchmarks/graph_truth": "benchmarks/tracks/graph_truth",
    "benchmarks/configs/patch_runner_external_agent.local.toml": "benchmarks/tracks/swebench_lite/configs/patch_runner_external_agent.local.toml",
    "benchmarks/configs/example.local.toml": "benchmarks/tracks/internal_gold/configs/example.local.toml",
}


# Ignored local payloads keep legacy fallbacks so existing setup remains usable
# during the compatibility window. Canonical paths are always preferred when they
# exist; old local payloads are only reused when the new track-local payload has
# not been materialized yet.
CANONICAL_FALLBACKS = {
    "benchmarks/tracks/repobench/workspaces/repobench_data": "benchmarks/workspaces/repobench_data",
    "benchmarks/tracks/crosscodeeval/upstream/cceval": "benchmarks/upstream/cceval",
    "benchmarks/tracks/crosscodeeval/upstream/cceval/data": "benchmarks/upstream/cceval/data",
    "benchmarks/tracks/swebench_lite/upstream/SWE-bench": "benchmarks/upstream/SWE-bench",
    "benchmarks/tracks/swebench_lite/workspaces/benchmark-setup-venv": "benchmarks/workspaces/benchmark-setup-venv",
    "benchmarks/tracks/swebench_lite/workspaces/gold_validation": "benchmarks/workspaces/swebench_gold_validation",
}


def repo_root() -> Path:
    return REPO_ROOT


def benchmarks_root() -> Path:
    return BENCHMARKS_ROOT


def track_root(track_id: str) -> Path:
    _require_track(track_id)
    return TRACKS_ROOT / track_id


def config_path(track_id: str, name: str) -> Path:
    return track_root(track_id) / "configs" / name


def dataset_path(track_id: str, *parts: str) -> Path:
    return track_root(track_id).joinpath("datasets", *parts)


def fixture_path(track_id: str, *parts: str) -> Path:
    return track_root(track_id).joinpath("fixtures", *parts)


def workspace_path(track_id: str, *parts: str) -> Path:
    return track_root(track_id).joinpath("workspaces", *parts)


def results_path(track_id: str, *parts: str) -> Path:
    return track_root(track_id).joinpath("results", *parts)


def upstream_path(track_id: str, *parts: str) -> Path:
    return track_root(track_id).joinpath("upstream", *parts)


def script_path(track_id: str, *parts: str) -> Path:
    return track_root(track_id).joinpath("scripts", *parts)


def resolve_benchmark_path(value: str | Path, *, prefer_existing: bool = True) -> Path:
    path = Path(value)
    if path.is_absolute():
        try:
            normalized = path.relative_to(REPO_ROOT).as_posix()
        except ValueError:
            return path
        candidates = [REPO_ROOT / candidate for candidate in _candidate_paths(normalized)]
        if prefer_existing:
            for candidate in candidates:
                if candidate.exists():
                    return candidate
        return candidates[0]
    normalized = path.as_posix()
    candidates = _candidate_paths(normalized)
    if prefer_existing:
        for candidate in candidates:
            if candidate.exists():
                return candidate
    return candidates[0]


def resolve_existing_benchmark_path(value: str | Path) -> Path:
    return resolve_benchmark_path(value, prefer_existing=True)


def _candidate_paths(normalized: str) -> list[Path]:
    reverse = {canonical: legacy for legacy, canonical in LEGACY_TO_CANONICAL.items()}
    if normalized in LEGACY_TO_CANONICAL:
        return [Path(LEGACY_TO_CANONICAL[normalized]), Path(normalized)]
    if normalized in CANONICAL_FALLBACKS:
        return [Path(normalized), Path(CANONICAL_FALLBACKS[normalized])]
    for canonical_root, fallback_root in CANONICAL_FALLBACKS.items():
        prefix = f"{canonical_root}/"
        if normalized.startswith(prefix):
            suffix = normalized[len(prefix) :]
            return [Path(normalized), Path(fallback_root).joinpath(*suffix.split("/"))]
    if normalized in reverse:
        return [Path(normalized), Path(reverse[normalized])]
    return [Path(normalized)]


def _require_track(track_id: str) -> None:
    if track_id not in TRACK_IDS:
        raise ValueError(f"unknown benchmark track: {track_id}")
