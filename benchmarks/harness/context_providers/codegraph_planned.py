from __future__ import annotations

import json
import re
import time
from collections import OrderedDict, defaultdict
from pathlib import Path
from typing import Any

from benchmarks.harness.command_runner import CommandRunner
from benchmarks.harness.context_providers.base import ContextPacket, ContextProvider, ProviderBudget, query_terms
from benchmarks.harness.workspace import resolve_repo_path, safe_slug, stable_id


GRAPH_PROOF_STRENGTHS = {"graph_relation_proof", "mutation_proof", "flow_proof"}
NON_PROOF_ROLES = {
    "critical_file": "text_evidence",
    "critical_symbol": "symbol_evidence",
    "text_evidence": "text_evidence",
    "fallback_snippet": "text_evidence",
    "source_navigation_evidence": "source_navigation_evidence",
    "query_file": "text_evidence",
    "query_symbol": "symbol_evidence",
    "query_text": "text_evidence",
    "candidate_evidence": "candidate_evidence",
    "no_proof_fallback": "text_evidence",
}
ROLE_PRIORITY = (
    "graph_relation_proof",
    "critical_file",
    "critical_symbol",
    "text_evidence",
    "source_navigation_evidence",
    "query_file",
    "query_symbol",
    "query_text",
    "fallback_snippet",
    "candidate_evidence",
    "no_proof_fallback",
)
CONFIG_NAMES = {"config.in", "cargo.toml", "package.json", "pyproject.toml", "tsconfig.json", "readme.md"}
TEXT_HINT_WORDS = {
    "docs",
    "documentation",
    "manual",
    "config",
    "schema",
    "error",
    "message",
    "trace",
    "accounting",
    "metric",
    "persisted",
    "lifecycle",
}
STOPWORDS = {
    "and",
    "the",
    "for",
    "find",
    "trace",
    "where",
    "with",
    "that",
    "this",
    "from",
    "into",
    "before",
    "after",
    "implementation",
    "package",
}


class CodeGraphPlannedProvider(ContextProvider):
    mode = "codegraph_planned"

    def get_context(self, task: dict, budget: ProviderBudget) -> ContextPacket:
        started = time.perf_counter()
        packet = ContextPacket(
            claimability={
                "provider": self.mode,
                "graph_proof": False,
                "proof_status": "unknown",
                "proof_strength": "no_evidence",
                "claimable": False,
                "diagnostic_only": False,
            }
        )
        if not self.release_binary or not self.release_binary.exists():
            packet.unknowns.append("release binary missing")
            packet.raw = {"provider": self.mode, "status": "missing_release_binary"}
            return packet

        repo = resolve_repo_path(task.get("repo_path"), self.repo_root)
        task_slug = safe_slug(str(task.get("task_id") or "task"))
        log_dir = self.workspace.resolve() / "logs" / "codegraph_planned" / task_slug
        runner = CommandRunner(log_dir)
        db, vector_path = self._artifact_paths(repo)
        command_records: list[dict[str, Any]] = []
        query_records: list[dict[str, Any]] = []
        planned_queries: list[dict[str, Any]] = []
        context_pack_json: dict[str, Any] = {}
        context_stdout_bytes = 0
        query_stdout_bytes = 0
        index_ms = 0
        context_ms = 0
        query_ms = 0
        normal_dot_codegraph_before = (self.repo_root / ".codegraph").exists()

        db_prebuilt = db.exists()
        if not db_prebuilt:
            index_record = self._run_index(runner, repo, db, task, budget)
            command_records.append(index_record.to_dict())
            packet.tool_calls += 1
            index_ms += index_record.wall_time_ms
            if not index_record.success:
                packet.unknowns.append(f"codegraph index/prebuild failed: exit {index_record.exit_code}")
                packet.raw = {
                    "schema_version": "codegraph_planned_context_v1",
                    "provider": self.mode,
                    "repo": str(repo),
                    "db_path": str(db),
                    "prebuilt_db_used": False,
                    "command_log_path": str(log_dir / "commands.jsonl"),
                    "failure_kind": index_record.failure_kind,
                    "stderr_tail": _read_tail(Path(index_record.stderr_path)),
                }
                return packet

        context_record = self._run_context_pack(runner, repo, db, vector_path, task, budget)
        command_records.append(context_record.to_dict())
        packet.tool_calls += 1
        context_ms += context_record.wall_time_ms
        context_stdout_bytes += context_record.stdout_bytes
        if context_record.success:
            context_pack_json = _parse_json_file(Path(context_record.stdout_path)) or {}
        else:
            packet.unknowns.append(f"codegraph context-pack failed: exit {context_record.exit_code}")

        planned_queries = build_planned_queries(task, context_pack_json)
        query_budget = _query_budget(task, budget, packet.tool_calls)
        queries_to_run = _select_queries_for_budget(planned_queries, query_budget)
        for index, query in enumerate(queries_to_run, 1):
            record = self._run_query(runner, repo, db, query, index, budget)
            command_records.append(record.to_dict())
            query_record = {
                **query,
                "command_id": record.command_id,
                "exit_code": record.exit_code,
                "success": record.success,
                "stdout_path": record.stdout_path,
                "stderr_path": record.stderr_path,
                "stdout_bytes": record.stdout_bytes,
                "stderr_bytes": record.stderr_bytes,
                "wall_time_ms": record.wall_time_ms,
                "failure_kind": record.failure_kind,
            }
            query_records.append(query_record)
            packet.tool_calls += 1
            query_ms += record.wall_time_ms
            query_stdout_bytes += record.stdout_bytes

        candidates = extract_context_pack_candidates(context_pack_json)
        for record in query_records:
            if not record.get("success"):
                continue
            query_json = _parse_json_file(Path(record["stdout_path"])) or {}
            candidates.extend(extract_query_candidates(query_json, record))

        selected = _ensure_source_navigation_for_implementation_trace(
            role_diverse_rank(candidates, limit=_evidence_limit(task, budget)),
            task,
        )
        _fill_packet(packet, selected, context_pack_json, task)
        raw_total_ms = int((time.perf_counter() - started) * 1000)
        packet.raw_context_bytes = context_stdout_bytes + query_stdout_bytes
        packet.raw = {
            "schema_version": "codegraph_planned_context_v1",
            "provider": self.mode,
            "repo": str(repo),
            "db_path": str(db),
            "vector_path": str(vector_path),
            "prebuilt_db_used": db_prebuilt,
            "prebuild_executed": not db_prebuilt,
            "vector_artifact_used": vector_path.exists(),
            "candidate_spool_used": bool(_candidate_spool_path(task)),
            "command_log_path": str(log_dir / "commands.jsonl"),
            "task_intent": _routing_value(context_pack_json, "task_intent"),
            "task_profile": _routing_value(context_pack_json, "task_profile"),
            "retrieval_plan_summary": _routing_value(context_pack_json, "retrieval_plan_summary"),
            "routing_packet_fields_used": bool(_routing_packet(context_pack_json)),
            "planned_queries": planned_queries,
            "queries_selected_for_execution": queries_to_run,
            "executed_queries": query_records,
            "selected_role_counts": _role_counts(selected),
            "source_navigation_evidence": [
                item for item in selected if item.get("proof_strength") == "source_navigation_evidence"
            ],
            "candidate_evidence": [
                item for item in selected if item.get("proof_strength") == "candidate_evidence"
            ],
            "artifact_db_inspection_requirements": _artifact_db_requirements(context_pack_json, task),
            "packet_fields": {
                "proof_status": packet.claimability.get("proof_status"),
                "proof_strength": packet.claimability.get("proof_strength"),
                "graph_proof": packet.claimability.get("graph_proof"),
                "omitted_count": _omitted_count(context_pack_json),
                "diagnostic_only": bool(context_pack_json.get("diagnostic_only", False)),
            },
            "codegraph_subprocess_timing_ms": {
                "query_subprocess_ms": query_ms,
                "context_pack_subprocess_ms": context_ms,
                "index_prebuild_ms": index_ms,
                "wall_total_ms": raw_total_ms,
            },
            "commands": command_records,
            "normal_dot_codegraph_before": normal_dot_codegraph_before,
            "normal_dot_codegraph_after": (self.repo_root / ".codegraph").exists(),
            "normal_dot_codegraph_mutated": normal_dot_codegraph_before != (self.repo_root / ".codegraph").exists(),
            "claim_boundary": "local diagnostic provider packet; no public benchmark claim",
        }
        packet.raw["claimability"] = packet.claimability
        return packet

    def metadata(self) -> dict:
        return {
            "mode": self.mode,
            "uses_rg": False,
            "uses_codegraph": True,
            "uses_external_agent": False,
            "uses_task_intent": True,
            "uses_retrieval_plan": True,
            "uses_context_pack_routing_packet": True,
            "vector_candidates_enabled": "if_prebuilt_vector_artifact_exists",
            "candidate_spool_enabled": "if_configured",
            "graph_proof": "only_when_graph_source_verification_succeeds",
        }

    def _artifact_paths(self, repo: Path) -> tuple[Path, Path]:
        key = stable_id(str(repo))
        workspace = self.workspace.resolve()
        db = workspace / "dbs" / f"{key}.sqlite"
        vector = workspace / "vectors" / f"{key}.json"
        db.parent.mkdir(parents=True, exist_ok=True)
        vector.parent.mkdir(parents=True, exist_ok=True)
        return db, vector

    def _run_index(
        self,
        runner: CommandRunner,
        repo: Path,
        db: Path,
        task: dict,
        budget: ProviderBudget,
    ):
        argv = [str(self.release_binary), "index", str(repo), "--db", str(db), "--fresh", "--json"]
        return runner.run(
            argv,
            cwd=self.repo_root,
            timeout_s=_timeout_s(task, budget, default=180),
            command_id="codegraph_planned_prebuild_index",
        )

    def _run_context_pack(
        self,
        runner: CommandRunner,
        repo: Path,
        db: Path,
        vector_path: Path,
        task: dict,
        budget: ProviderBudget,
    ):
        task_text = _task_text(task)
        argv = [
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
            str(budget.max_context_bytes or task.get("max_output_bytes") or 60000),
        ]
        spool_path = _candidate_spool_path(task)
        if spool_path:
            argv.extend(["--candidate-spool", spool_path, "--early-candidates"])
        if vector_path.exists():
            argv.extend(["--enable-vector-candidates", "--vector-index", str(vector_path)])
        return runner.run(
            argv,
            cwd=self.repo_root,
            timeout_s=_timeout_s(task, budget, default=180),
            command_id="codegraph_planned_context_pack",
        )

    def _run_query(
        self,
        runner: CommandRunner,
        repo: Path,
        db: Path,
        query: dict[str, Any],
        index: int,
        budget: ProviderBudget,
    ):
        kind = query["kind"]
        term = query["term"]
        argv = [
            str(self.release_binary),
            "--repo",
            str(repo),
            "--db",
            str(db),
            "query",
            kind,
            term,
            "--agent-json",
            "--limit",
            str(query.get("limit") or 8),
        ]
        return runner.run(
            argv,
            cwd=self.repo_root,
            timeout_s=_timeout_s(query, budget, default=60),
            command_id=f"codegraph_planned_query_{index:02d}_{kind}_{safe_slug(term)[:32]}",
        )


def build_planned_queries(task: dict, context_pack_json: dict[str, Any]) -> list[dict[str, Any]]:
    clues = _visible_clues(task)
    queries: list[dict[str, Any]] = []
    for clue in clues:
        term = clue["term"]
        if _is_path_like(term) or _is_filename_like(term) or clue["source"] in {"visible_file_hint", "stack_frame"}:
            queries.append(_query("files", term, clue, "path/name clue routes to query files", "high"))
        if _is_symbol_like(term) or clue["source"] == "visible_symbol_hint":
            queries.append(_query("symbols", term, clue, "symbol-like clue routes to query symbols", "medium"))
        if _is_text_like(term, task) or clue["source"] in {"visible_text_hint", "config_key", "error_message"}:
            queries.append(_query("text", term, clue, "prose/docs/config clue routes to query text", "medium"))

    for follow_up in _follow_up_queries(context_pack_json):
        term = str(follow_up.get("query_text") or "").strip()
        if not term:
            continue
        kind = "files" if _is_path_like(term) or str(follow_up.get("path_scope") or "") else "text"
        queries.append(
            {
                "kind": kind,
                "term": term,
                "source": "routing_packet_follow_up",
                "why": str(follow_up.get("why") or "structured follow-up query from routing packet"),
                "expected_signal": str(follow_up.get("expected_signal") or "routing-packet suggested evidence"),
                "path_scope": str(follow_up.get("path_scope") or ""),
                "flood_risk": str(follow_up.get("risk") or "medium"),
                "limit": int(follow_up.get("max_results_hint") or 8) if str(follow_up.get("max_results_hint") or "").isdigit() else 8,
            }
        )

    return _dedupe_queries(queries)


def extract_context_pack_candidates(data: dict[str, Any]) -> list[dict[str, Any]]:
    candidates: list[dict[str, Any]] = []
    routing = _routing_packet(data)
    for item in _list_value(routing, "verified_paths") + _list_value(data, "proof_paths"):
        candidates.append(normalize_codegraph_evidence_item(item, role="graph_relation_proof", source="context_pack"))
    for item in _list_value(routing, "critical_files") + _list_value(data, "likely_files") + _list_value(data, "critical_files"):
        candidates.append(normalize_codegraph_evidence_item(item, role="critical_file", source="routing_packet"))
    for item in _list_value(routing, "critical_symbols") + _list_value(data, "critical_symbols") + _list_value(data, "symbols"):
        candidates.append(normalize_codegraph_evidence_item(item, role="critical_symbol", source="routing_packet"))
    for item in _list_value(routing, "text_evidence") + _list_value(data, "text_evidence") + _list_value(data, "fallback_evidence"):
        candidates.append(normalize_codegraph_evidence_item(item, role="text_evidence", source="routing_packet"))
    for item in _list_value(routing, "source_navigation_evidence") + _list_value(data, "source_navigation_evidence"):
        candidates.append(normalize_codegraph_evidence_item(item, role="source_navigation_evidence", source="routing_packet"))
    for item in _list_value(routing, "fallback_snippets") + _list_value(data, "fallback_snippets") + _list_value(data, "snippets"):
        candidates.append(normalize_codegraph_evidence_item(item, role="fallback_snippet", source="context_pack"))
    for item in _list_value(data, "candidates"):
        candidates.append(normalize_codegraph_evidence_item(item, role="candidate_evidence", source="context_pack"))
    return [candidate for candidate in candidates if candidate.get("file") or candidate.get("symbol") or candidate.get("text")]


def extract_query_candidates(data: dict[str, Any], query_record: dict[str, Any]) -> list[dict[str, Any]]:
    role = {"files": "query_file", "symbols": "query_symbol", "text": "query_text"}.get(str(query_record.get("kind")), "query_text")
    candidates = []
    for item in _list_value(data, "results"):
        candidate = normalize_codegraph_evidence_item(item, role=role, source="query")
        candidate["query_term"] = query_record.get("term")
        candidate["query_kind"] = query_record.get("kind")
        candidate["command_id"] = query_record.get("command_id")
        candidates.append(candidate)
    return candidates


def normalize_codegraph_evidence_item(item: Any, *, role: str, source: str) -> dict[str, Any]:
    if isinstance(item, str):
        if role in {"critical_symbol", "query_symbol"}:
            item_dict: dict[str, Any] = {"symbol": item}
        elif role in {"critical_file", "query_file"}:
            item_dict = {"file": item}
        else:
            item_dict = {"text": item}
    elif isinstance(item, dict):
        item_dict = dict(item)
    else:
        item_dict = {"text": str(item)}

    file_path = _first_string(item_dict, "file", "path", "file_path")
    span = item_dict.get("span") if isinstance(item_dict.get("span"), dict) else item_dict.get("source_span")
    if isinstance(span, dict) and not file_path:
        file_path = _first_string(span, "file", "path", "file_path")
    snippet_obj = item_dict.get("snippet") if isinstance(item_dict.get("snippet"), dict) else {}
    if not file_path and isinstance(snippet_obj, dict):
        file_path = _first_string(snippet_obj, "file", "path", "file_path")

    symbol = _first_string(item_dict, "symbol", "name", "resolved_symbol")
    text = _first_string(item_dict, "text", "snippet_text", "summary", "why", "reason")
    if not text and isinstance(snippet_obj, dict):
        text = _first_string(snippet_obj, "text", "snippet", "reason")

    start_line = item_dict.get("start_line") or item_dict.get("line")
    end_line = item_dict.get("end_line") or item_dict.get("line")
    if isinstance(span, dict):
        start_line = start_line or span.get("start_line")
        end_line = end_line or span.get("end_line")
    if isinstance(snippet_obj, dict):
        lines = snippet_obj.get("lines")
        start_line = start_line or _first_line(lines)
        end_line = end_line or _first_line(lines)

    proof_status = str(item_dict.get("proof_status") or item_dict.get("status") or "")
    candidate_source = str(item_dict.get("candidate_source") or item_dict.get("evidence_kind") or item_dict.get("evidence_role") or "")
    graph_verified = _graph_verified(item_dict, role, proof_status, candidate_source)
    if graph_verified:
        proof_strength = "graph_relation_proof"
        proof_status = proof_status if proof_status in {"proof_path_found", "verified"} else "verified"
        graph_proof = True
    else:
        proof_strength = _proof_strength(role, candidate_source)
        proof_status = proof_status or ("no_proof_path_found" if proof_strength != "candidate_evidence" else "candidate_only")
        graph_proof = False

    result = {
        "role": role,
        "source": source,
        "file": _norm_path(file_path),
        "symbol": symbol,
        "text": text,
        "start_line": _maybe_int(start_line),
        "end_line": _maybe_int(end_line),
        "score": _score(item_dict, role),
        "evidence_type": proof_strength,
        "proof_strength": proof_strength,
        "proof_status": proof_status,
        "graph_proof": graph_proof,
        "candidate_source": candidate_source,
        "diagnostic_only": proof_strength == "candidate_evidence" or bool(item_dict.get("diagnostic_only", False)),
        "claimability": _claimability(item_dict, graph_proof, proof_strength, proof_status),
    }
    if proof_strength == "candidate_evidence":
        result["candidate_only"] = True
    return result


def role_diverse_rank(candidates: list[dict[str, Any]], *, limit: int) -> list[dict[str, Any]]:
    seen: set[tuple[Any, ...]] = set()
    buckets: OrderedDict[str, list[dict[str, Any]]] = OrderedDict((role, []) for role in ROLE_PRIORITY)
    for candidate in sorted(candidates, key=lambda item: (-float(item.get("score") or 0), str(item.get("file") or ""))):
        key = (
            candidate.get("role"),
            candidate.get("file"),
            candidate.get("symbol"),
            candidate.get("start_line"),
            candidate.get("text"),
        )
        if key in seen:
            continue
        seen.add(key)
        buckets.setdefault(str(candidate.get("role") or "text_evidence"), []).append(candidate)

    selected: list[dict[str, Any]] = []
    while len(selected) < limit:
        added = False
        for bucket in buckets.values():
            if not bucket:
                continue
            selected.append(bucket.pop(0))
            added = True
            if len(selected) >= limit:
                break
        if not added:
            break
    return selected


def _ensure_source_navigation_for_implementation_trace(
    selected: list[dict[str, Any]],
    task: dict,
) -> list[dict[str, Any]]:
    task_type = str(task.get("task_type") or "").lower()
    if task_type != "implementation_trace":
        return selected
    if any(item.get("proof_strength") == "source_navigation_evidence" for item in selected):
        return selected
    for item in selected:
        if not item.get("file"):
            continue
        derived = {
            **item,
            "role": "source_navigation_evidence",
            "source": "derived_from_planned_codegraph_candidates",
            "evidence_type": "source_navigation_evidence",
            "proof_strength": "source_navigation_evidence",
            "proof_status": "no_proof_path_found",
            "graph_proof": False,
            "claimability": {
                "graph_proof": False,
                "proof_strength": "source_navigation_evidence",
                "proof_status": "no_proof_path_found",
                "claimable_for_graph": False,
                "not_claimable_as": ["graph_relation_proof", "artifact_value_proof"],
            },
        }
        return [derived, *selected]
    return selected


def _fill_packet(packet: ContextPacket, selected: list[dict[str, Any]], data: dict[str, Any], task: dict) -> None:
    files: list[str] = []
    symbols: list[str] = []
    spans: list[dict[str, Any]] = []
    snippets: list[dict[str, Any]] = []
    text_evidence: list[dict[str, Any]] = []
    proof_paths: list[dict[str, Any]] = []
    graph_proof_allowed = _graph_proof_allowed(task)
    for item in selected:
        item_graph_proof = bool(item.get("graph_proof")) and graph_proof_allowed
        if item.get("file") and item["file"] not in files:
            files.append(item["file"])
        if item.get("symbol") and item["symbol"] not in symbols:
            symbols.append(item["symbol"])
        if item.get("file") and item.get("start_line") is not None:
            spans.append({"file": item["file"], "start_line": item["start_line"], "end_line": item.get("end_line")})
        if item.get("text") or item.get("file"):
            snippet = {
                "file": item.get("file"),
                "symbol": item.get("symbol"),
                "start_line": item.get("start_line"),
                "end_line": item.get("end_line"),
                "text": item.get("text"),
                "role": item.get("role"),
                "proof_strength": item.get("proof_strength"),
                "proof_status": item.get("proof_status"),
                "graph_proof": item_graph_proof,
                "candidate_source": item.get("candidate_source"),
            }
            snippets.append(snippet)
            if item.get("proof_strength") in {"text_evidence", "source_navigation_evidence", "candidate_evidence"}:
                text_evidence.append(snippet)
        if item_graph_proof:
            proof_paths.append({**item, "graph_proof": True})

    routing = _routing_packet(data)
    packet.files = files
    packet.symbols = symbols
    packet.spans = spans
    packet.snippets = snippets
    packet.text_evidence = text_evidence
    packet.proof_paths = proof_paths
    packet.unknowns = _list_value(routing, "unknowns") + _list_value(data, "unknowns")
    packet.risks = _list_value(routing, "risks") + _list_value(data, "risks")
    packet.validation_steps = _list_value(routing, "validation_steps") + _artifact_db_requirements(data, task)
    packet.follow_up_queries = _follow_up_queries(data)
    claim = _packet_claimability(data, selected, task)
    packet.claimability.update(claim)
    if claim.get("proof_status") in {"no_proof_path_found", "no_evidence_found"} and not packet.unknowns:
        packet.unknowns.append(
            {
                "claim": "graph relation proof",
                "reason": claim.get("proof_status"),
                "sentence": "No graph proof is claimed from text, symbol, candidate, or source-navigation evidence.",
            }
        )


def _packet_claimability(data: dict[str, Any], selected: list[dict[str, Any]], task: dict) -> dict[str, Any]:
    routing_claim = _routing_value(data, "claimability")
    graph_proof = _graph_proof_allowed(task) and any(bool(item.get("graph_proof")) for item in selected)
    evidence = bool(selected)
    if graph_proof:
        return {
            "provider": "codegraph_planned",
            "graph_proof": True,
            "proof_status": "verified",
            "proof_strength": "graph_relation_proof",
            "claimable": True,
            "diagnostic_only": False,
        }
    proof_status = str(
        (routing_claim or {}).get("proof_status")
        or data.get("proof_status")
        or ("no_proof_path_found" if evidence else "no_evidence_found")
    )
    if proof_status in {"proof_path_found", "verified"}:
        proof_status = "no_proof_path_found"
    strengths = {str(item.get("proof_strength") or "") for item in selected}
    if "source_navigation_evidence" in strengths:
        strength = "source_navigation_evidence"
    elif "text_evidence" in strengths:
        strength = "text_evidence"
    elif "symbol_evidence" in strengths:
        strength = "symbol_evidence"
    elif "candidate_evidence" in strengths:
        strength = "candidate_evidence"
    elif "graph_relation_proof" in strengths:
        strength = "source_navigation_evidence"
    else:
        strength = "no_evidence"
    return {
        "provider": "codegraph_planned",
        "graph_proof": False,
        "proof_status": proof_status,
        "proof_strength": strength,
        "claimable": bool((routing_claim or {}).get("claimable", bool(evidence and strength == "text_evidence"))),
        "claimable_for_graph": False,
        "diagnostic_only": bool(data.get("diagnostic_only", False)),
        "not_claimable_as": ["graph_relation_proof"],
    }


def _graph_proof_allowed(task: dict[str, Any]) -> bool:
    expected = task.get("expected_claimability") if isinstance(task.get("expected_claimability"), dict) else {}
    if expected.get("graph_proof_allowed") is False:
        return False
    forbidden = {str(value) for value in _iter_values(task.get("forbidden_claim_types"))}
    if forbidden.intersection({"graph_proof", "graph_relation_proof", "behavior_proof", "artifact_value_proof"}):
        return False
    expected_status = str(task.get("expected_proof_status") or expected.get("expected_proof_status") or "")
    if expected_status == "no_proof_path_found":
        return False
    return True


def _visible_clues(task: dict) -> list[dict[str, str]]:
    entries: list[dict[str, str]] = []
    provenance = task.get("visible_query_term_sources", [])
    for term in query_terms(task):
        entries.append({"term": term, "source": _source_for_term(term, provenance)})
    for token in _prompt_tokens(_task_text(task)):
        entries.append({"term": token, "source": "prompt"})
    for field, source in (
        ("visible_file_hints", "visible_file_hint"),
        ("visible_symbol_hints", "visible_symbol_hint"),
        ("visible_text_hints", "visible_text_hint"),
        ("visible_config_keys", "config_key"),
        ("visible_error_messages", "error_message"),
        ("visible_stack_frames", "stack_frame"),
    ):
        for value in _iter_values(task.get(field)):
            entries.append({"term": value, "source": source})
    return _dedupe_clues(entries)


def _query(kind: str, term: str, clue: dict[str, str], why: str, flood_risk: str) -> dict[str, Any]:
    return {
        "kind": kind,
        "term": term,
        "source": clue.get("source", "prompt"),
        "why": why,
        "expected_signal": {
            "files": "file/path matches",
            "symbols": "typed or indexed symbol matches",
            "text": "source text snippets and docs/config evidence",
        }[kind],
        "path_scope": _path_scope(term),
        "flood_risk": flood_risk,
        "limit": 8,
    }


def _dedupe_queries(queries: list[dict[str, Any]]) -> list[dict[str, Any]]:
    seen: set[tuple[str, str]] = set()
    result: list[dict[str, Any]] = []
    priority = {"files": 0, "symbols": 1, "text": 2}
    for query in sorted(queries, key=lambda item: (priority.get(item["kind"], 9), len(str(item["term"])))):
        key = (str(query["kind"]), str(query["term"]).lower())
        if key in seen:
            continue
        seen.add(key)
        result.append(query)
    return result[:12]


def _select_queries_for_budget(queries: list[dict[str, Any]], budget: int) -> list[dict[str, Any]]:
    if budget <= 0:
        return []
    selected: list[dict[str, Any]] = []
    used: set[int] = set()
    for kind in ("files", "symbols", "text"):
        for index, query in enumerate(queries):
            if index in used or query.get("kind") != kind:
                continue
            selected.append(query)
            used.add(index)
            break
        if len(selected) >= budget:
            return selected
    for index, query in enumerate(queries):
        if index in used:
            continue
        selected.append(query)
        if len(selected) >= budget:
            break
    return selected


def _dedupe_clues(entries: list[dict[str, str]]) -> list[dict[str, str]]:
    seen: set[str] = set()
    result: list[dict[str, str]] = []
    source_priority = {
        "visible_file_hint": 0,
        "stack_frame": 0,
        "visible_symbol_hint": 1,
        "config_key": 1,
        "visible_seed": 1,
        "prompt": 2,
        "visible_text_hint": 2,
        "error_message": 3,
    }
    for entry in sorted(entries, key=lambda item: source_priority.get(item.get("source", ""), 5)):
        term = _clean_term(entry.get("term", ""))
        if len(term) < 2:
            continue
        if term.lower() in STOPWORDS and entry.get("source") not in {"visible_file_hint", "visible_symbol_hint"}:
            continue
        key = term.replace("\\", "/").lower()
        if key in seen:
            continue
        seen.add(key)
        result.append({"term": term, "source": entry.get("source") or "prompt"})
        if len(result) >= 16:
            break
    return result


def _prompt_tokens(text: str) -> list[str]:
    tokens = re.findall(r"[\w./\\-]+\.[A-Za-z0-9_]+|[A-Za-z_][A-Za-z0-9_]{2,}", text)
    return [_clean_term(token) for token in tokens if len(_clean_term(token)) > 2]


def _is_path_like(term: str) -> bool:
    normalized = term.replace("\\", "/")
    return "/" in normalized or bool(Path(normalized).suffix)


def _is_filename_like(term: str) -> bool:
    lower = Path(term.replace("\\", "/")).name.lower()
    return lower in CONFIG_NAMES or "." in lower


def _is_symbol_like(term: str) -> bool:
    if _is_path_like(term):
        return False
    if len(term) < 3:
        return False
    if not re.match(r"^[A-Za-z_][A-Za-z0-9_:-]*$", term):
        return False
    return "_" in term or "::" in term or any(ch.isupper() for ch in term[1:]) or term.isupper()


def _is_text_like(term: str, task: dict) -> bool:
    lower = f"{term} {_task_text(task)}".lower()
    if any(word in lower for word in TEXT_HINT_WORDS):
        return True
    if " " in term or "-" in term:
        return True
    return not _is_path_like(term) and not _is_symbol_like(term)


def _path_scope(term: str) -> str:
    normalized = term.replace("\\", "/")
    if "/" in normalized:
        parent = str(Path(normalized).parent).replace("\\", "/")
        return parent if parent != "." else ""
    if Path(normalized).suffix:
        return f"*{Path(normalized).suffix}"
    return ""


def _graph_verified(item: dict[str, Any], role: str, proof_status: str, candidate_source: str) -> bool:
    if role != "graph_relation_proof":
        return False
    if _candidate_only_source(candidate_source):
        return False
    return item.get("graph_proof") is True and proof_status in {"proof_path_found", "verified"}


def _proof_strength(role: str, candidate_source: str) -> str:
    if _candidate_only_source(candidate_source) or role == "candidate_evidence":
        return "candidate_evidence"
    return NON_PROOF_ROLES.get(role, "text_evidence")


def _candidate_only_source(candidate_source: str) -> bool:
    lowered = candidate_source.lower()
    return any(token in lowered for token in ("vector", "candidate", "nuance", "binary"))


def _claimability(item: dict[str, Any], graph_proof: bool, proof_strength: str, proof_status: str) -> dict[str, Any]:
    raw = item.get("claimability") if isinstance(item.get("claimability"), dict) else {}
    claimability = dict(raw)
    claimability.update(
        {
            "graph_proof": graph_proof,
            "proof_strength": proof_strength,
            "proof_status": proof_status,
            "claimable_for_graph": graph_proof,
        }
    )
    if not graph_proof:
        claimability.setdefault("not_claimable_as", ["graph_relation_proof"])
    return claimability


def _score(item: dict[str, Any], role: str) -> float:
    base = {
        "graph_relation_proof": 1000.0,
        "critical_file": 600.0,
        "critical_symbol": 550.0,
        "source_navigation_evidence": 480.0,
        "text_evidence": 420.0,
        "query_file": 360.0,
        "query_symbol": 340.0,
        "query_text": 320.0,
        "fallback_snippet": 280.0,
        "candidate_evidence": 120.0,
    }.get(role, 100.0)
    try:
        return base + float(item.get("score") or 0)
    except (TypeError, ValueError):
        return base


def _packet_values(data: dict[str, Any], key: str) -> list[Any]:
    routing = _routing_packet(data)
    return _list_value(routing, key) + _list_value(data, key)


def _routing_packet(data: dict[str, Any]) -> dict[str, Any]:
    value = data.get("routing_packet") if isinstance(data, dict) else None
    return value if isinstance(value, dict) else {}


def _routing_value(data: dict[str, Any], key: str) -> Any:
    routing = _routing_packet(data)
    if key in routing:
        return routing.get(key)
    arch = data.get("retrieval_architecture") if isinstance(data.get("retrieval_architecture"), dict) else {}
    return arch.get(key) if isinstance(arch, dict) else None


def _list_value(data: dict[str, Any], key: str) -> list[Any]:
    value = data.get(key) if isinstance(data, dict) else None
    return value if isinstance(value, list) else []


def _follow_up_queries(data: dict[str, Any]) -> list[dict[str, Any]]:
    values = _packet_values(data, "follow_up_queries")
    return [item for item in values if isinstance(item, dict)]


def _artifact_db_requirements(data: dict[str, Any], task: dict) -> list[dict[str, Any]]:
    routing = _routing_packet(data)
    requirements = []
    for key in ("artifact_inspection_requirements", "db_inspection_requirements"):
        for item in _list_value(routing, key):
            if isinstance(item, dict):
                requirements.append({**item, "source": f"routing_packet.{key}"})
            else:
                requirements.append({"description": str(item), "source": f"routing_packet.{key}"})
    task_type = str(task.get("task_type") or "").lower()
    text = _task_text(task).lower()
    if task_type == "implementation_trace" or any(word in text for word in ("persisted", "artifact", "db", "accounting", "metric")):
        requirements.append(
            {
                "kind": "artifact_or_db_inspection_required",
                "description": "Inspect generated DB/vector artifacts or status output before claiming final persisted values.",
                "reason": "source-navigation evidence cannot prove final artifact values by itself",
                "graph_proof": False,
                "proof_strength": "source_navigation_evidence",
            }
        )
    return requirements


def _omitted_count(data: dict[str, Any]) -> int:
    value = data.get("omitted_count") if isinstance(data, dict) else 0
    try:
        return int(value or 0)
    except (TypeError, ValueError):
        return 0


def _role_counts(items: list[dict[str, Any]]) -> dict[str, int]:
    counts: defaultdict[str, int] = defaultdict(int)
    for item in items:
        counts[str(item.get("role") or "unknown")] += 1
    return dict(counts)


def _query_budget(task: dict, budget: ProviderBudget, tool_calls_so_far: int) -> int:
    max_calls = budget.max_tool_calls or int(task.get("max_tool_calls") or 8)
    return max(0, int(max_calls) - tool_calls_so_far)


def _evidence_limit(task: dict, budget: ProviderBudget) -> int:
    max_bytes = budget.max_context_bytes or int(task.get("max_output_bytes") or 60000)
    return 12 if max_bytes >= 40000 else 8


def _timeout_s(task: dict, budget: ProviderBudget, *, default: int) -> int:
    return max(1, min(default, int(budget.max_time_s or (int(task.get("max_wall_time_ms") or 0) // 1000) or default)))


def _candidate_spool_path(task: dict) -> str:
    for source in (task, task.get("metadata") if isinstance(task.get("metadata"), dict) else {}, task.get("visible_repo_metadata") if isinstance(task.get("visible_repo_metadata"), dict) else {}):
        value = source.get("candidate_spool_path") if isinstance(source, dict) else None
        if value:
            return str(value)
    return ""


def _parse_json_file(path: Path) -> dict[str, Any] | None:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return None
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


def _task_text(task: dict) -> str:
    for key in ("task", "task_text", "prompt"):
        value = task.get(key)
        if isinstance(value, str) and value.strip():
            return value
    return ""


def _source_for_term(term: str, provenance: Any) -> str:
    if isinstance(provenance, list):
        for item in provenance:
            if isinstance(item, dict) and str(item.get("term", "")).lower() == str(term).lower():
                return str(item.get("source") or "visible_seed")
    return "visible_seed"


def _iter_values(value: Any) -> list[str]:
    if value is None:
        return []
    if isinstance(value, str):
        return [value]
    if isinstance(value, list):
        return [str(item) for item in value if str(item).strip()]
    if isinstance(value, dict):
        return [str(item) for item in value.values() if str(item).strip()]
    return [str(value)]


def _clean_term(value: str) -> str:
    return str(value).strip(" \t\r\n.,:;()[]{}\"'`")


def _first_string(data: dict[str, Any], *keys: str) -> str:
    for key in keys:
        value = data.get(key)
        if isinstance(value, str) and value:
            return value
    return ""


def _first_line(value: Any) -> int | None:
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        match = re.search(r"\d+", value)
        if match:
            return int(match.group(0))
    return None


def _maybe_int(value: Any) -> int | None:
    if value is None or value == "":
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _norm_path(value: str) -> str:
    return str(value or "").replace("\\", "/")


def _read_tail(path: Path, max_chars: int = 1200) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")[-max_chars:]
    except OSError:
        return ""
