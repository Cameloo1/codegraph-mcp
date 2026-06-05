from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import subprocess
from pathlib import Path
from typing import Any

from benchmarks.harness.adapters.swebench_adapter import SWEBenchAdapter
from benchmarks.harness.config import load_config
from benchmarks.harness.paths import resolve_benchmark_path

DEFAULT_EXTERNAL_AGENT_COMMAND_ENV = "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND"
DEFAULT_EXTERNAL_AGENT_CONFIG = "benchmarks/tracks/swebench_lite/configs/smoke.toml"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", default="benchmarks/tracks/swebench_lite/configs/smoke.toml")
    parser.add_argument("--output", default=None)
    args = parser.parse_args(argv)
    summary = patch_setup_summary(args.config)
    if args.output:
        path = Path(args.output)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 0


def patch_setup_summary(config_path: str) -> dict:
    config = load_config(config_path)
    adapter_cfg = config.raw.get("adapter", {})
    agent_cfg = config.raw.get("agent", {})
    upstream_repo = resolve_benchmark_path(adapter_cfg.get("upstream_repo", "benchmarks/tracks/swebench_lite/upstream/SWE-bench"))
    swebench = SWEBenchAdapter(upstream_repo)
    swe_status = swebench.setup_status().to_dict()
    command, command_env, command_source = resolve_external_agent_command(agent_cfg)
    external_agent = validate_external_agent_command(command, command_env, agent_cfg)
    external_agent_status = external_agent["status"]
    quality_status = "ready" if swe_status["ready"] and external_agent_status == "ready" else "blocked"
    blockers = []
    if not swe_status["ready"]:
        blockers.extend(swe_status["blockers"])
    if external_agent_status != "ready":
        blocker = external_agent.get("blocker") or f"external_agent_command status is {external_agent_status}."
        blockers.append(f"External agent command is not ready. {blocker}")
    return {
        "schema_version": "patch_runner_setup_v1",
        "status": quality_status,
        "claim_boundary": "mock_agent is scaffold_only; real patch quality requires external_agent_command and official-compatible harness",
        "swebench": swe_status,
        "external_agent_command": {
            "status": external_agent_status,
            "env": command_env,
            "configured": bool(command),
            "source": command_source,
            "validation": external_agent,
        },
        "mock_agent": {"status": "scaffold_only", "quality": False},
        "blockers": blockers,
    }


def resolve_external_agent_command(
    agent_cfg: dict[str, Any] | None = None,
    *,
    explicit_command: str | None = None,
) -> tuple[str, str, str]:
    agent_cfg = agent_cfg or {}
    command_env = str(agent_cfg.get("external_agent_command_env", DEFAULT_EXTERNAL_AGENT_COMMAND_ENV))
    if explicit_command is not None:
        stripped = explicit_command.strip()
        return stripped, command_env, "explicit_cli" if stripped else "explicit_empty"
    env_command = os.environ.get(command_env, "").strip()
    if env_command:
        return env_command, command_env, "env"
    config_command = str(agent_cfg.get("external_agent_command", "")).strip()
    if config_command:
        return config_command, command_env, "config"
    return "", command_env, "missing"


def resolve_external_agent_command_from_config(
    config_path: str | Path = DEFAULT_EXTERNAL_AGENT_CONFIG,
    *,
    explicit_command: str | None = None,
) -> tuple[str, str, str, dict[str, Any]]:
    config = load_config(config_path)
    agent_cfg = config.raw.get("agent", {})
    command, env_name, source = resolve_external_agent_command(agent_cfg, explicit_command=explicit_command)
    return command, env_name, source, agent_cfg


def validate_external_agent_command(command: str, command_env: str, agent_cfg: dict[str, Any]) -> dict[str, Any]:
    command = command.strip()
    if not command:
        return {
            "status": "blocked_not_configured",
            "env": command_env,
            "configured": False,
            "blocker": f"Set {command_env} or config agent.external_agent_command.",
        }
    try:
        parts = shlex.split(command, posix=False)
    except ValueError as exc:
        return {
            "status": "blocked_invalid_command",
            "env": command_env,
            "configured": True,
            "blocker": f"external_agent_command is not parseable: {exc}",
        }
    if not parts:
        return {
            "status": "blocked_invalid_command",
            "env": command_env,
            "configured": True,
            "blocker": "external_agent_command parsed to an empty command.",
        }
    executable = parts[0].strip('"')
    resolved = shutil.which(executable)
    if resolved is None and not Path(executable).exists():
        return {
            "status": "blocked_command_not_found",
            "env": command_env,
            "configured": True,
            "executable": executable,
            "blocker": f"external_agent_command executable was not found: {executable}",
        }
    dry_run_args = agent_cfg.get("external_agent_dry_run_args")
    if dry_run_args:
        dry_parts = list(parts)
        if isinstance(dry_run_args, str):
            dry_parts.extend(shlex.split(dry_run_args, posix=False))
        elif isinstance(dry_run_args, list):
            dry_parts.extend(str(item) for item in dry_run_args)
        try:
            proc = subprocess.run(dry_parts, text=True, capture_output=True, check=False, timeout=30)
        except Exception as exc:  # validation should report, not throw
            return {
                "status": "blocked_dry_run_failed",
                "env": command_env,
                "configured": True,
                "executable": executable,
                "blocker": f"external_agent_command dry-run failed to start: {exc}",
            }
        if proc.returncode != 0:
            return {
                "status": "blocked_dry_run_failed",
                "env": command_env,
                "configured": True,
                "executable": executable,
                "exit_code": proc.returncode,
                "stdout_bytes": len((proc.stdout or "").encode("utf-8", errors="ignore")),
                "stderr_bytes": len((proc.stderr or "").encode("utf-8", errors="ignore")),
                "blocker": "external_agent_command dry-run returned non-zero.",
            }
    return {
        "status": "ready",
        "env": command_env,
        "configured": True,
        "executable": executable,
        "resolved": resolved or str(Path(executable)),
        "required_env": list(agent_cfg.get("required_env", [])),
    }


if __name__ == "__main__":
    raise SystemExit(main())
