from __future__ import annotations

import json
import re
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
        timing = {
            "query_subprocess_ms": 0,
            "context_pack_subprocess_ms": 0,
            "index_prebuild_ms": 0,
        }
        workspace = self._workspace_root()
        repo = resolve_repo_path(task.get("repo_path"), self.repo_root)
        db = workspace / "dbs" / f"{stable_id(str(repo))}.sqlite"
        db.parent.mkdir(parents=True, exist_ok=True)
        if not db.exists():
            index_cmd = [str(self.release_binary), "index", str(repo), "--db", str(db), "--fresh", "--json"]
            record = run_command(index_cmd, self.repo_root, workspace / "logs" / f"codegraph_index_{stable_id(str(repo))}.json", timeout_s=budget.max_time_s or 120)
            timing["index_prebuild_ms"] += record.wall_time_ms
            packet.tool_calls += 1
            if record.exit_code != 0:
                packet.unknowns.append(f"codegraph index failed: exit {record.exit_code}")
                packet.raw = {"stderr": record.stderr[-1000:], "codegraph_subprocess_timing_ms": timing}
                return packet
        vector_path = workspace / "vectors" / f"{stable_id(str(repo))}.json"
        if full:
            self._ensure_vector_sidecar(packet, repo, db, vector_path, workspace, budget)
        self._run_targeted_queries(packet, task, budget, repo, db, workspace, timing)
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
            if vector_path.exists():
                context_cmd.extend(["--enable-vector-candidates", "--vector-index", str(vector_path)])
            context_cmd.append("--enable-nuance-rescue-candidates")
        record = run_command(context_cmd, self.repo_root, workspace / "logs" / f"codegraph_context_{task['task_id']}_{self.mode}.json", timeout_s=budget.max_time_s or 120)
        timing["context_pack_subprocess_ms"] += record.wall_time_ms
        packet.tool_calls += 1
        packet.raw_context_bytes += len(record.stdout.encode("utf-8", errors="ignore"))
        if record.exit_code != 0:
            packet.unknowns.append(f"codegraph context-pack failed: exit {record.exit_code}")
            packet.raw = {"stderr": record.stderr[-1000:], "codegraph_subprocess_timing_ms": timing}
            return packet
        parsed = _parse_json(record.stdout)
        if parsed is not None:
            _merge_codegraph_json(packet, parsed)
        _rerank_for_task_profile(packet, task)
        packet.raw = {"provider": self.mode, "db": str(db), "codegraph_subprocess_timing_ms": timing}
        return packet

    def _workspace_root(self) -> Path:
        if self.workspace.is_absolute():
            return self.workspace
        return self.repo_root / self.workspace

    def _ensure_vector_sidecar(
        self,
        packet: ContextPacket,
        repo: Path,
        db: Path,
        vector_path: Path,
        workspace: Path,
        budget: ProviderBudget,
    ) -> None:
        if vector_path.exists():
            return
        max_calls = budget.max_tool_calls or 10
        if packet.tool_calls >= max_calls - 1:
            packet.risks.append({"kind": "vector_sidecar_skipped", "reason": "tool_call_budget_reserved_for_context"})
            return
        vector_path.parent.mkdir(parents=True, exist_ok=True)
        vector_cmd = [
            str(self.release_binary),
            "index",
            str(repo),
            "--db",
            str(db),
            "--json",
            "--build-vector-index",
            str(vector_path),
        ]
        timeout_s = min(budget.max_time_s or 120, 60)
        record = run_command(
            vector_cmd,
            self.repo_root,
            workspace / "logs" / f"codegraph_vector_{stable_id(str(repo))}.json",
            timeout_s=timeout_s,
        )
        packet.tool_calls += 1
        packet.raw_context_bytes += len(record.stdout.encode("utf-8", errors="ignore"))
        if record.exit_code != 0:
            packet.risks.append(
                {
                    "kind": "vector_sidecar_unavailable",
                    "exit_code": record.exit_code,
                    "reason": "vector candidates are optional and remain non-proof",
                }
            )

    def _run_targeted_queries(
        self,
        packet: ContextPacket,
        task: dict,
        budget: ProviderBudget,
        repo: Path,
        db: Path,
        workspace: Path,
        timing: dict,
    ) -> None:
        max_calls = budget.max_tool_calls or 10
        remaining_for_queries = max(0, max_calls - packet.tool_calls - 1)
        if remaining_for_queries <= 0:
            return
        calls_used = 0
        for term in query_terms(task):
            for query_kind in _query_routes_for_term(term):
                if calls_used >= remaining_for_queries:
                    return
                query_cmd = [
                    str(self.release_binary),
                    "--repo",
                    str(repo),
                    "--db",
                    str(db),
                    "query",
                    query_kind,
                    term,
                    "--agent-json",
                    "--limit",
                    "8",
                ]
                log_name = f"codegraph_query_{task['task_id']}_{query_kind}_{stable_id(term)}.json"
                q = run_command(query_cmd, self.repo_root, workspace / "logs" / log_name, timeout_s=budget.max_time_s or 120)
                timing["query_subprocess_ms"] += q.wall_time_ms
                packet.tool_calls += 1
                calls_used += 1
                packet.raw_context_bytes += len(q.stdout.encode("utf-8", errors="ignore"))
                qjson = _parse_json(q.stdout)
                if qjson is not None:
                    _merge_codegraph_json(packet, qjson)


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

    for key in ("results", "files", "likely_files", "critical_files", "candidates", "fallback_evidence"):
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
        for key in ("source_navigation_evidence", "text_evidence", "fallback_snippets"):
            values = routing.get(key, [])
            if isinstance(values, list):
                packet.text_evidence.extend(values)
                for item in values:
                    if isinstance(item, dict):
                        path = item.get("file") or item.get("path") or item.get("file_path")
                        if path:
                            add_file(str(path))
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


def _query_routes_for_term(term: str) -> list[str]:
    value = str(term).strip()
    if not value:
        return []
    routes: list[str] = []

    def add(route: str) -> None:
        if route not in routes:
            routes.append(route)

    if _looks_symbol_like(value):
        add("symbols")
        add("text")
    elif any(separator in value for separator in ("/", "\\", ".")):
        add("files")
        add("text")
    elif any(separator in value for separator in (" ", "-", "_")):
        add("text")
    else:
        add("text")
    return routes


def _looks_symbol_like(term: str) -> bool:
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", term):
        return False
    has_lower = any(ch.islower() for ch in term)
    has_upper = any(ch.isupper() for ch in term)
    return "_" in term or (has_lower and has_upper)


def _rerank_for_task_profile(packet: ContextPacket, task: dict) -> None:
    if task.get("repo_kind") != "codegraph_self":
        return
    if task.get("task_type") not in {"db_lifecycle_debug", "implementation_trace", "routing_packet"}:
        return
    packet.files.sort(key=_self_repo_implementation_file_priority)


def _self_repo_implementation_file_priority(path: str) -> tuple[int, int, str]:
    normalized = str(path).replace("\\", "/").lstrip("./")
    if normalized.startswith("crates/") and "/src/" in normalized:
        return (0, 0, normalized)
    if normalized.startswith("codegraph-ui/") and "/src/" in normalized:
        return (0, 1, normalized)
    if normalized.startswith("fixtures/") or normalized.startswith("benchmarks/"):
        return (1, 0, normalized)
    if normalized.startswith("docs/") or normalized.startswith("reports/") or normalized.endswith("/README.md") or normalized == "README.md":
        return (3, 0, normalized)
    return (2, 0, normalized)
