from __future__ import annotations

import shutil
from pathlib import Path

from benchmarks.harness.context_providers.base import ContextPacket, ContextProvider, ProviderBudget, query_terms
from benchmarks.harness.logging_utils import run_command
from benchmarks.harness.workspace import resolve_repo_path, stable_id


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
        for index, term in enumerate(query_terms(task)[:max_calls]):
            log_path = self.workspace / "logs" / f"rg_{task['task_id']}_{index}_{stable_id(term)}.json"
            cmd = [
                str(rg),
                "--no-ignore",
                "-n",
                "--glob",
                "!.git/**",
                "--glob",
                "!target/**",
                "--glob",
                "!node_modules/**",
                "--glob",
                "!benchmarks/results/**",
                "--glob",
                "!reports/audit/artifacts/**",
                "--glob",
                "!.codegraph/**",
                term,
                ".",
            ]
            record = run_command(cmd, repo, log_path=log_path, timeout_s=budget.max_time_s)
            packet.tool_calls += 1
            stdout = record.stdout or ""
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
        packet.raw = {"provider": self.mode}
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
