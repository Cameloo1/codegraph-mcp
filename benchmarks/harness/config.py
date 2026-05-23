from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from benchmarks.harness.paths import resolve_benchmark_path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    tomllib = None  # type: ignore[assignment]


@dataclass(frozen=True)
class BenchmarkConfig:
    name: str
    dataset: str
    dataset_path: Path
    dataset_version: str
    modes: list[str]
    max_tasks: int | None
    max_time_s: int | None
    max_tool_calls: int | None
    max_context_bytes: int
    max_output_bytes: int
    claim_boundary: str
    release_binary: Path
    workspace_dir: Path
    binary_path_policy: str
    db_path_policy: str
    agent_scaffold: str
    model: str
    external_agent_required: bool
    raw: dict[str, Any]


def load_config(path: str | Path, _seen: set[str] | None = None) -> BenchmarkConfig:
    if tomllib is None:
        raise RuntimeError("tomllib is required; use Python 3.11+")
    config_path = resolve_benchmark_path(path)
    seen = _seen or set()
    config_key = config_path.as_posix()
    if config_key in seen:
        raise RuntimeError(f"cyclic benchmark config alias: {config_path}")
    seen.add(config_key)
    with config_path.open("rb") as handle:
        raw = tomllib.load(handle)
    compat = raw.get("compat", {})
    if isinstance(compat, dict) and compat.get("canonical_config"):
        return load_config(str(compat["canonical_config"]), seen)
    run = raw.get("run", {})
    codegraph = raw.get("codegraph", {})
    agent = raw.get("agent", {})
    return BenchmarkConfig(
        name=str(run.get("name", config_path.stem)),
        dataset=str(run.get("dataset", "internal_gold")),
        dataset_path=resolve_benchmark_path(str(run.get("dataset_path", ""))),
        dataset_version=str(run.get("dataset_version", run.get("dataset_name", ""))),
        modes=list(run.get("modes", ["none", "rg_only"])),
        max_tasks=_maybe_int(run.get("max_tasks")),
        max_time_s=_maybe_int(run.get("max_time_s", run.get("max_wall_time_s"))),
        max_tool_calls=_maybe_int(run.get("max_tool_calls")),
        max_context_bytes=int(run.get("max_context_bytes", 60000)),
        max_output_bytes=int(run.get("max_output_bytes", run.get("max_context_bytes", 60000))),
        claim_boundary=str(run.get("claim_boundary", "diagnostic")),
        release_binary=resolve_benchmark_path(str(codegraph.get("release_binary", "target/release/codegraph-mcp.exe")), prefer_existing=False),
        workspace_dir=resolve_benchmark_path(str(codegraph.get("workspace_dir", f"benchmarks/tracks/{run.get('dataset', 'internal_gold')}/workspaces/{config_path.stem}")), prefer_existing=False),
        binary_path_policy=str(codegraph.get("binary_path_policy", "release_binary_required")),
        db_path_policy=str(codegraph.get("db_path_policy", "ignored_benchmark_workspace")),
        agent_scaffold=str(agent.get("scaffold", "retrieval_only")),
        model=str(agent.get("model", "none")),
        external_agent_required=bool(agent.get("external_agent_required", False)),
        raw=raw,
    )


def _maybe_int(value: Any) -> int | None:
    if value is None:
        return None
    return int(value)


def validate_config(config: BenchmarkConfig) -> list[str]:
    errors: list[str] = []
    valid_modes = {
        "none",
        "baseline",
        "rg_only",
        "rg_planned",
        "codegraph_exact_text",
        "codegraph_full",
        "codegraph_current",
        "codegraph_planned",
    }
    valid_claims = {"diagnostic", "official_compatible", "official", "scaffold_only"}
    if config.dataset not in {"internal_gold", "repobench", "crosscodeeval", "swe_bench_lite"}:
        errors.append(f"unsupported dataset adapter: {config.dataset}")
    for mode in config.modes:
        if mode not in valid_modes:
            errors.append(f"unsupported mode: {mode}")
    if config.claim_boundary not in valid_claims:
        errors.append(f"unsupported claim_boundary: {config.claim_boundary}")
    if config.binary_path_policy != "release_binary_required":
        errors.append("CodeGraph binary path policy must be release_binary_required")
    if config.db_path_policy != "ignored_benchmark_workspace":
        errors.append("DB path policy must be ignored_benchmark_workspace")
    workspace = config.workspace_dir.as_posix()
    if "benchmarks/workspaces" not in workspace and "benchmarks/tracks/" not in workspace:
        errors.append("workspace_dir must be under benchmarks/workspaces or benchmarks/tracks/<track>/workspaces")
    if config.max_context_bytes <= 0 or config.max_output_bytes <= 0:
        errors.append("context/output byte budgets must be positive")
    return errors
