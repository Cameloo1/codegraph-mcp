from __future__ import annotations

import json
from pathlib import Path

from benchmarks.harness.context_providers.base import ContextPacket, ContextProvider, ProviderBudget, query_terms
from benchmarks.harness.logging_utils import run_command
from benchmarks.harness.workspace import resolve_repo_path, stable_id


class CodeGraphExactTextProvider(ContextProvider):
    mode = "codegraph_exact_text"

    def get_context(self, task: dict, budget: ProviderBudget) -> ContextPacket:
        return self._run_codegraph(task, budget, full=False)

    def metadata(self) -> dict:
        return {
            "mode": self.mode,
            "uses_rg": False,
            "uses_codegraph": True,
            "uses_external_agent": False,
            "vector_candidates_enabled": False,
            "nuance_candidates_enabled": False,
        }

    def _run_codegraph(self, task: dict, budget: ProviderBudget, full: bool) -> ContextPacket:
        packet = ContextPacket(claimability={"graph_proof": False, "provider": self.mode})
        if not self.release_binary or not self.release_binary.exists():
            packet.unknowns.append("release binary missing")
            return packet
        repo = resolve_repo_path(task.get("repo_path"), self.repo_root)
        db = self.workspace / "dbs" / f"{stable_id(str(repo))}.sqlite"
        db.parent.mkdir(parents=True, exist_ok=True)
        if not db.exists():
            index_cmd = [str(self.release_binary), "index", str(repo), "--db", str(db), "--fresh", "--json"]
            if full:
                vector_path = self.workspace / "vectors" / f"{stable_id(str(repo))}.json"
                vector_path.parent.mkdir(parents=True, exist_ok=True)
                index_cmd.extend(["--build-vector-index", str(vector_path)])
            record = run_command(index_cmd, self.repo_root, self.workspace / "logs" / f"codegraph_index_{stable_id(str(repo))}.json", timeout_s=budget.max_time_s or 120)
            packet.tool_calls += 1
            if record.exit_code != 0:
                packet.unknowns.append(f"codegraph index failed: exit {record.exit_code}")
                packet.raw = {"stderr": record.stderr[-1000:]}
                return packet
        task_text = str(task.get("task", ""))
        context_cmd = [
            str(self.release_binary),
            "--repo",
            str(repo),
            "--db",
            str(db),
            "context-pack",
            "--task",
            task_text,
            "--agent-json",
            "--max-output-bytes",
            str(budget.max_context_bytes),
        ]
        if full:
            vector_path = self.workspace / "vectors" / f"{stable_id(str(repo))}.json"
            if vector_path.exists():
                context_cmd.extend(["--enable-vector-candidates", "--vector-index", str(vector_path)])
            context_cmd.append("--enable-nuance-rescue-candidates")
        record = run_command(context_cmd, self.repo_root, self.workspace / "logs" / f"codegraph_context_{task['task_id']}_{self.mode}.json", timeout_s=budget.max_time_s or 120)
        packet.tool_calls += 1
        packet.raw_context_bytes += len(record.stdout.encode("utf-8", errors="ignore"))
        if record.exit_code != 0:
            packet.unknowns.append(f"codegraph context-pack failed: exit {record.exit_code}")
            packet.raw = {"stderr": record.stderr[-1000:]}
            return packet
        parsed = _parse_json(record.stdout)
        if parsed is not None:
            _merge_codegraph_json(packet, parsed)
        for term in query_terms(task)[:2]:
            query_cmd = [
                str(self.release_binary),
                "--repo",
                str(repo),
                "--db",
                str(db),
                "query",
                "files",
                term,
                "--agent-json",
                "--limit",
                "8",
            ]
            q = run_command(query_cmd, self.repo_root, self.workspace / "logs" / f"codegraph_query_{task['task_id']}_{stable_id(term)}.json", timeout_s=budget.max_time_s or 120)
            packet.tool_calls += 1
            packet.raw_context_bytes += len(q.stdout.encode("utf-8", errors="ignore"))
            qjson = _parse_json(q.stdout)
            if qjson is not None:
                _merge_codegraph_json(packet, qjson)
        packet.raw = {"provider": self.mode, "db": str(db)}
        return packet


def _parse_json(text: str) -> dict | None:
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        start = text.find("{")
        end = text.rfind("}")
        if start >= 0 and end > start:
            try:
                return json.loads(text[start : end + 1])
            except json.JSONDecodeError:
                return None
    return None


def _merge_codegraph_json(packet: ContextPacket, data: dict) -> None:
    def add_file(value: str) -> None:
        normalized = value.replace("\\", "/")
        if normalized and normalized not in packet.files:
            packet.files.append(normalized)

    for key in ("results", "files", "likely_files", "critical_files"):
        values = data.get(key, [])
        if isinstance(values, list):
            for item in values:
                if isinstance(item, str):
                    add_file(item)
                elif isinstance(item, dict):
                    path = item.get("file") or item.get("path") or item.get("file_path")
                    if path:
                        add_file(str(path))
                    if item.get("symbol"):
                        packet.symbols.append(item.get("symbol"))
    for key in ("snippets", "fallback_snippets", "text_evidence", "source_navigation_evidence"):
        values = data.get(key, [])
        if isinstance(values, list):
            packet.snippets.extend(values)
            if key != "snippets":
                packet.text_evidence.extend(values)
            for item in values:
                if isinstance(item, dict):
                    path = item.get("file") or item.get("path") or item.get("file_path")
                    if path:
                        add_file(str(path))
    routing = data.get("routing_packet")
    if isinstance(routing, dict):
        for key in ("critical_files", "likely_files"):
            values = routing.get(key, [])
            if isinstance(values, list):
                for item in values:
                    if isinstance(item, str):
                        add_file(item)
                    elif isinstance(item, dict):
                        path = item.get("file") or item.get("path")
                        if path:
                            add_file(str(path))
        for key in ("source_navigation_evidence", "text_evidence"):
            values = routing.get(key, [])
            if isinstance(values, list):
                packet.text_evidence.extend(values)
    proof_paths = data.get("proof_paths")
    if isinstance(proof_paths, list):
        packet.proof_paths.extend(proof_paths)
    packet.claimability.update(
        {
            "graph_proof": bool(data.get("graph_proof", False)),
            "proof_status": data.get("proof_status"),
            "claimable": data.get("claimable"),
        }
    )
