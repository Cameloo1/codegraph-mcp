from __future__ import annotations

import json
import os
import sys
import subprocess
from pathlib import Path
from typing import Any

from benchmarks.harness.adapters.base import AdapterSetupStatus


class SWEBenchAdapter:
    benchmark = "swe_bench_lite"
    dataset_version = "princeton-nlp/SWE-bench_Lite"

    def __init__(self, upstream_repo: str | Path):
        self.upstream_repo = Path(upstream_repo)

    def setup_status(self) -> AdapterSetupStatus:
        blockers: list[str] = []
        commands = [
            "python -m swebench.harness.run_evaluation --dataset_name princeton-nlp/SWE-bench_Lite --predictions_path gold --max_workers 1 --instance_ids sympy__sympy-20590 --run_id validate-gold"
        ]
        details: dict[str, Any] = {}
        if not self.upstream_repo.exists():
            blockers.append("Pinned SWE-bench upstream checkout is missing.")
            return AdapterSetupStatus(
                benchmark=self.benchmark,
                setup_state="skipped_with_precise_blocker",
                status="blocked_unknown",
                dataset_version=self.dataset_version,
                local_path=str(self.upstream_repo),
                blockers=blockers,
                setup_commands=[
                    "git clone https://github.com/SWE-bench/SWE-bench benchmarks/upstream/SWE-bench",
                    "git -C benchmarks/upstream/SWE-bench checkout f7bbbb2ccdf479001d6467c9e34af59e44a840f9",
                ],
            )
        docker_status = _docker_status()
        details["docker"] = docker_status
        if docker_status["status"] != "ready":
            blockers.append(docker_status["blocker"])
        host_status = _host_harness_status()
        details["host_harness"] = host_status
        linux_status = _linux_harness_status()
        details["linux_harness"] = linux_status
        install_status = _python_import_status()
        details["python_import"] = install_status
        if install_status["status"] != "ready":
            blockers.append(install_status["blocker"])
        harness = self.upstream_repo / "swebench" / "harness" / "run_evaluation.py"
        if not harness.exists():
            blockers.append("SWE-bench harness file swebench/harness/run_evaluation.py is missing from the checkout.")
        harness_ready = host_status["status"] == "ready" or linux_status["status"] == "ready"
        if not harness_ready:
            blockers.append(linux_status["blocker"] or host_status["blocker"])
        state = "partial_smoke_ready" if self.upstream_repo.exists() else "skipped_with_precise_blocker"
        if docker_status["status"] != "ready":
            status = "blocked_docker"
        elif install_status["status"] != "ready":
            status = "blocked_python_install"
        elif not harness_ready:
            status = "hard_external_prerequisite_linux_harness"
        elif linux_status["status"] == "ready":
            status = "ready_for_gold_validation"
        else:
            status = "ready_for_gold_validation"
        return AdapterSetupStatus(
            benchmark=self.benchmark,
            setup_state=state if blockers else "ready_for_official_smoke",
            status=status,
            dataset_version=self.dataset_version,
            local_path=str(self.upstream_repo),
            ready=not blockers,
            smoke_ready=self.upstream_repo.exists(),
            fixture_only=False,
            blockers=blockers,
            setup_commands=commands,
            notes=[
                "SWE-bench source checkout, package install, Docker access, and Linux harness path are tracked separately.",
                "Gold validation can run through the Linux-container route when the recorded gold report exists or the setup script is rerun.",
                "Patch quality still requires a real external agent command; mock-agent runs remain scaffold-only.",
            ],
            details=details,
        )

    def validate_gold_available(self) -> tuple[bool, str]:
        status = self.setup_status()
        if status.ready:
            return True, "SWE-bench upstream and Docker are ready for gold-patch validation."
        return False, "; ".join(status.blockers)

    def load_tasks(self, limit: int | None = None) -> list[dict]:
        ok, reason = self.validate_gold_available()
        if not ok:
            raise RuntimeError(reason)
        return []


def _docker_status() -> dict[str, Any]:
    env = os.environ.copy()
    docker_config = Path("benchmarks/workspaces/docker-config").resolve()
    docker_config.mkdir(parents=True, exist_ok=True)
    env.setdefault("DOCKER_CONFIG", str(docker_config))
    env.setdefault("DOCKER_HOST", "npipe:////./pipe/dockerDesktopLinuxEngine")
    try:
        version = subprocess.run(["docker", "--version"], text=True, capture_output=True, check=False, timeout=20, env=env)
    except FileNotFoundError:
        return {"status": "blocked_docker", "blocker": "Docker CLI not found.", "version": None}
    except subprocess.TimeoutExpired:
        return {"status": "blocked_docker", "blocker": "Docker CLI version check timed out.", "version": None}
    info = subprocess.run(["docker", "info", "--format", "{{json .}}"], text=True, capture_output=True, check=False, timeout=30, env=env)
    if info.returncode != 0:
        return {
            "status": "blocked_docker",
            "blocker": (info.stderr or info.stdout or "docker info failed").strip(),
            "version": (version.stdout or version.stderr).strip(),
        }
    parsed = None
    try:
        parsed = json.loads(info.stdout)
    except json.JSONDecodeError:
        parsed = None
    return {
        "status": "ready",
        "blocker": "",
        "version": (version.stdout or version.stderr).strip(),
        "info": parsed,
    }


def _host_harness_status() -> dict[str, Any]:
    try:
        import resource  # type: ignore[import-not-found]
    except ModuleNotFoundError:
        return {
            "status": "blocked_host_platform",
            "blocker": (
                "Official SWE-bench harness imports Python's Unix resource module; "
                f"current host Python {sys.version.split()[0]} on {sys.platform} does not provide it. "
                "Run the harness from WSL/Linux or a Linux host."
            ),
        }
    return {"status": "ready", "blocker": ""}


def _python_import_status() -> dict[str, Any]:
    try:
        import importlib.util

        spec = importlib.util.find_spec("swebench")
    except Exception:
        spec = None
    if spec is None:
        venv_python = Path("benchmarks/workspaces/benchmark-setup-venv/Scripts/python.exe")
        if venv_python.exists():
            proc = subprocess.run(
                [
                    str(venv_python),
                    "-c",
                    "import importlib.util, json; spec=importlib.util.find_spec('swebench'); print(json.dumps({'module': None if spec is None else spec.origin}))",
                ],
                text=True,
                capture_output=True,
                check=False,
                timeout=30,
            )
            if proc.returncode == 0:
                try:
                    parsed = json.loads(proc.stdout.strip())
                except json.JSONDecodeError:
                    parsed = {"stdout": proc.stdout.strip()}
                if parsed.get("module"):
                    return {"status": "ready", "blocker": "", "via": str(venv_python), **parsed}
        return {
            "status": "blocked_python_install",
            "blocker": "SWE-bench package is not importable. Run python -m pip install -e benchmarks/upstream/SWE-bench.",
        }
    return {
        "status": "ready",
        "blocker": "",
        "module": spec.origin,
    }


def _linux_harness_status() -> dict[str, Any]:
    report = Path("benchmarks/workspaces/swebench_gold_validation/gold.codegraph-setup-gold.json")
    if report.exists():
        try:
            parsed = json.loads(report.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            parsed = {}
        if parsed.get("completed_instances", 0) >= 1 and "sympy__sympy-20590" in parsed.get("completed_ids", []):
            return {
                "status": "ready",
                "blocker": "",
                "route": "docker_linux_container",
                "gold_validation_report": str(report),
                "completed_instances": parsed.get("completed_instances"),
                "resolved_instances": parsed.get("resolved_instances"),
            }
    return {
        "status": "hard_external_prerequisite_linux_harness",
        "blocker": (
            "Run benchmarks/scripts/run_swebench_harness_linux_container.ps1 to validate the official harness "
            "through a Linux container or run benchmarks/scripts/setup_swebench_wsl.ps1 after installing a WSL distro."
        ),
        "route": "docker_linux_container_or_wsl",
    }
