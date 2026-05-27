from __future__ import annotations

import shutil
import time
from pathlib import Path

from benchmarks.harness.context_providers.base import ContextPacket, ContextProvider, ProviderBudget, query_terms
from benchmarks.harness.command_runner import CommandRunner
from benchmarks.harness.workspace import resolve_repo_path, stable_id


EXCLUDE_GLOBS = (
    "!.git/**",
    "!target/**",
    "!node_modules/**",
    "!benchmarks/results/**",
    "!benchmarks/workspaces/**",
    "!benchmarks/upstream/**",
    "!reports/audit/**",
    "!.codegraph/**",
)


class RgOnlyProvider(ContextProvider):
    mode = "rg_only"

    def get_context(self, task: dict, budget: ProviderBudget) -> ContextPacket:
        started = time.perf_counter()
        repo = resolve_repo_path(task.get("repo_path"), self.repo_root)
        rg = self._find_rg()
        packet = ContextPacket(claimability={"graph_proof": False, "text_evidence_only": True})
        if not rg:
            packet.unknowns.append("rg not found")
            return packet
        max_calls = budget.max_tool_calls or 4
        max_stdout_bytes = min(1_000_000, max(64_000, int((budget.max_context_bytes or 60_000) * 4)))
        runner = CommandRunner(self.workspace / "logs" / "rg_only" / str(task["task_id"]))
        commands: list[dict] = []
        for index, term in enumerate(query_terms(task)[:max_calls]):
            command_id = f"rg_{index}_{stable_id(term)}"
            cmd = [
                str(rg),
                "--no-ignore",
                "-n",
                "--max-filesize",
                "1M",
                "--max-count",
                "20",
            ]
            for glob in EXCLUDE_GLOBS:
                cmd.extend(["--glob", glob])
            cmd.extend(
                [
                    "--",
                    term,
                    ".",
                ]
            )
            record = runner.run(
                cmd,
                cwd=repo,
                timeout_s=min(int(budget.max_time_s or 20), 20),
                command_id=command_id,
                max_stdout_bytes=max_stdout_bytes,
                max_stderr_bytes=64_000,
            )
            commands.append(
                {
                    "command_id": command_id,
                    "kind": "line_evidence",
                    "argv": cmd,
                    "query_term": term,
                    "exit_code": record.exit_code,
                    "success": record.success,
                    "stdout_path": record.stdout_path,
                    "stderr_path": record.stderr_path,
                    "stdout_bytes": record.stdout_bytes,
                    "stderr_bytes": record.stderr_bytes,
                    "wall_time_ms": record.wall_time_ms,
                    "failure_kind": record.failure_kind,
                }
            )
            packet.tool_calls += 1
            stdout = _read_text(Path(record.stdout_path))
            text = stdout[: budget.max_context_bytes]
            packet.raw_context_bytes += len(text.encode("utf-8", errors="ignore"))
            for line in text.splitlines():
                parts = line.split(":", 2)
                if len(parts) >= 2:
                    file_path = parts[0].replace("\\", "/")
                    if file_path not in packet.files:
                        packet.files.append(file_path)
                    packet.snippets.append({"file": file_path, "line": parts[1], "text": parts[2] if len(parts) > 2 else ""})
            if len(stdout.encode("utf-8", errors="ignore")) > budget.max_context_bytes:
                packet.risks.append({"kind": "rg_flood", "term": term, "omitted_bytes": len(stdout) - len(text)})
            if record.failure_kind == "output_limit":
                packet.risks.append({"kind": "rg_stdout_cap", "term": term, "cap_bytes": max_stdout_bytes})
        packet.raw = {
            "schema_version": "rg_only_context_v1",
            "provider": self.mode,
            "repo": str(repo),
            "command_log_path": str(runner.commands_jsonl),
            "commands": commands,
            "metrics": {
                "tool_calls": packet.tool_calls,
                "stdout_bytes_total": sum(int(command.get("stdout_bytes") or 0) for command in commands),
                "stderr_bytes_total": sum(int(command.get("stderr_bytes") or 0) for command in commands),
                "context_bytes": packet.raw_context_bytes,
                "wall_time_ms": int((time.perf_counter() - started) * 1000),
                "command_wall_time_ms": sum(int(command.get("wall_time_ms") or 0) for command in commands),
            },
            "claimability": packet.claimability,
        }
        return packet

    def metadata(self) -> dict:
        return {
            "mode": self.mode,
            "uses_rg": True,
            "uses_codegraph": False,
            "uses_external_agent": False,
        }

    def _find_rg(self) -> Path | None:
        local = self.repo_root / ".codex-tools" / "rg.exe"
        if local.exists():
            return local
        found = shutil.which("rg")
        return Path(found) if found else None


def _read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return ""
