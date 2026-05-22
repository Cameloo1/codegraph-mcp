from __future__ import annotations

import json
import os
import shlex
import subprocess
from pathlib import Path

from benchmarks.harness.agent_scaffolds.base import AgentResult, AgentScaffold


class ExternalAgentCommand(AgentScaffold):
    name = "external_agent_command"
    env_var = "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND"
    legacy_env_var = "CODEGRAPH_BENCH_AGENT_COMMAND"

    def __init__(self, command: list[str] | None = None, cwd: str | Path | None = None):
        self.command = command or _command_from_env()
        self.cwd = Path(cwd) if cwd else Path.cwd()

    def run(self, task: dict, context: dict) -> AgentResult:
        if not self.command:
            return AgentResult(status="blocked_not_configured", quality_claim=False)
        proc = subprocess.run(
            self.command,
            input=json.dumps({"task": task, "context": context}),
            text=True,
            capture_output=True,
            cwd=str(self.cwd),
            check=False,
        )
        return AgentResult(
            status="ok" if proc.returncode == 0 else f"exit_{proc.returncode}",
            patch=proc.stdout if proc.stdout.startswith("diff --git") else None,
            raw_output=proc.stdout + proc.stderr,
            quality_claim=True,
        )


def _command_from_env() -> list[str] | None:
    value = os.environ.get(ExternalAgentCommand.env_var)
    if not value:
        value = os.environ.get(ExternalAgentCommand.legacy_env_var)
    if not value:
        return None
    return shlex.split(value, posix=False)
