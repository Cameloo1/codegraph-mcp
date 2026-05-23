from __future__ import annotations

import shutil
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
        repo = resolve_repo_path(task.get("repo_path"), self.repo_root)
        rg = self._find_rg()
        packet = ContextPacket(claimability={"graph_proof": False, "text_evidence_only": True})
        if not rg:
            packet.unknowns.append("rg not found")
            return packet
        max_calls = budget.max_tool_calls or 4
        max_stdout_bytes = min(1_000_000, max(64_000, int((budget.max_context_bytes or 60_000) * 4)))
        runner = CommandRunner(self.workspace / "logs" / "rg_only" / str(task["task_id"]))
        for index, term in enumerate(query_terms(task)[:max_calls]):
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
                command_id=f"rg_{index}_{stable_id(term)}",
                max_stdout_bytes=max_stdout_bytes,
                max_stderr_bytes=64_000,
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
        packet.raw = {"provider": self.mode, "command_log_path": str(runner.commands_jsonl)}
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
