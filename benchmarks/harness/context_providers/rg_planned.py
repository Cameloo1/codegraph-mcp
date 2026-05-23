from __future__ import annotations

import re
import shutil
import time
from collections import defaultdict
from pathlib import Path
from typing import Any

from benchmarks.harness.command_runner import CommandRunner
from benchmarks.harness.context_providers.base import ContextPacket, ContextProvider, ProviderBudget, query_terms
from benchmarks.harness.workspace import resolve_repo_path, safe_slug


EXCLUDE_GLOBS = (
    "!.git/**",
    "!target/**",
    "!node_modules/**",
    "!benchmarks/results/**",
    "!reports/audit/artifacts/**",
    "!.codegraph/**",
)

LANGUAGE_EXTENSIONS = {
    "rust": [".rs"],
    "python": [".py"],
    "typescript": [".ts", ".tsx"],
    "javascript": [".js", ".jsx"],
    "java": [".java"],
    "csharp": [".cs"],
    "c": [".c", ".h"],
    "markdown": [".md", ".markdown"],
}

CONFIG_FILENAMES = {
    "Config.in",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "tsconfig.json",
    "README.md",
}

STOPWORDS = {
    "and",
    "the",
    "for",
    "find",
    "trace",
    "open",
    "before",
    "after",
    "where",
    "with",
    "that",
    "this",
    "from",
    "into",
    "uses",
    "used",
    "file",
    "task",
    "source",
}


class RgPlannedProvider(ContextProvider):
    mode = "rg_planned"

    def get_context(self, task: dict, budget: ProviderBudget) -> ContextPacket:
        started = time.perf_counter()
        repo = resolve_repo_path(task.get("repo_path"), self.repo_root)
        rg = self._find_rg()
        packet = ContextPacket(
            claimability={
                "graph_proof": False,
                "claimable": False,
                "provider": self.mode,
                "proof_status": "not_graph_proof",
                "proof_strength": "text_evidence",
                "evidence_role": "lexical/text/path evidence",
                "text_evidence_only": True,
            }
        )
        if not rg:
            packet.unknowns.append("rg not found")
            packet.raw = {"provider": self.mode, "status": "missing_rg"}
            return packet

        limits = _limits(task, budget)
        plan = _build_plan(str(rg), task, limits)
        task_slug = safe_slug(str(task.get("task_id") or "task"))
        log_dir = self.workspace / "logs" / "rg_planned" / task_slug
        runner = CommandRunner(log_dir)
        state = _ProviderState(limits=limits)

        for command in plan:
            if state.tool_calls >= limits["max_commands"]:
                state.flood_events.append(
                    {
                        "kind": "max_commands_reached",
                        "limit": limits["max_commands"],
                        "skipped_command_id": command["command_id"],
                    }
                )
                command["skipped"] = True
                continue
            command["skipped"] = False
            record = runner.run(
                command["argv"],
                cwd=repo,
                timeout_s=limits["per_command_timeout_s"],
                command_id=command["command_id"],
            )
            state.tool_calls += 1
            command["exit_code"] = record.exit_code
            command["success"] = record.success
            command["stdout_path"] = record.stdout_path
            command["stderr_path"] = record.stderr_path
            command["stdout_bytes"] = record.stdout_bytes
            command["stderr_bytes"] = record.stderr_bytes
            command["wall_time_ms"] = record.wall_time_ms
            command["failure_kind"] = record.failure_kind
            stdout = _read_text(Path(record.stdout_path))
            if command["kind"] == "file_discovery":
                _consume_file_discovery(stdout, command, state)
            elif command["kind"] == "file_shortlist":
                _consume_file_shortlist(stdout, command, state)
            elif command["kind"] == "line_evidence":
                _consume_line_evidence(stdout, command, state)

        _add_ranked_files(packet, state)
        _add_snippets(packet, state)
        packet.raw_context_bytes = state.context_bytes
        packet.tool_calls = state.tool_calls
        packet.text_evidence = list(packet.snippets)
        packet.raw = {
            "schema_version": "rg_planned_context_v1",
            "provider": self.mode,
            "repo": str(repo),
            "command_log_path": str(log_dir / "commands.jsonl"),
            "plan": plan,
            "file_discovery": {
                "files_seen": state.files_seen,
                "candidate_files": _top_candidate_details(state, limits["max_files"]),
                "path_name_hits": state.path_name_hits,
            },
            "line_evidence": {
                "lines_seen": state.lines_seen,
                "lines_kept": len(state.snippets),
                "spans": state.spans,
            },
            "context_payload": {
                "bytes": state.context_bytes,
                "lines": len(state.snippets),
                "files": packet.files,
                "snippets": packet.snippets,
            },
            "flood_diagnostics": {
                "events": state.flood_events,
                "max_commands": limits["max_commands"],
                "max_files": limits["max_files"],
                "max_lines": limits["max_lines"],
                "max_bytes": limits["max_bytes"],
                "per_query_result_cap": limits["per_query_result_cap"],
                "noisy_probe_labels": sorted({event.get("label", "") for event in state.flood_events if event.get("label")}),
            },
            "metrics": {
                "tool_calls": state.tool_calls,
                "stdout_bytes_total": sum(int(command.get("stdout_bytes") or 0) for command in plan),
                "stderr_bytes_total": sum(int(command.get("stderr_bytes") or 0) for command in plan),
                "context_bytes": state.context_bytes,
                "context_lines": len(state.snippets),
                "wall_time_ms": int((time.perf_counter() - started) * 1000),
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
            "graph_proof": False,
            "evidence_role": "lexical/text/path evidence",
            "bounded": True,
        }

    def _find_rg(self) -> Path | None:
        local = self.repo_root / ".codex-tools" / "rg.exe"
        if local.exists():
            return local
        found = shutil.which("rg")
        return Path(found) if found else None


class _ProviderState:
    def __init__(self, *, limits: dict[str, int]):
        self.limits = limits
        self.tool_calls = 0
        self.files_seen = 0
        self.lines_seen = 0
        self.context_bytes = 0
        self.file_scores: defaultdict[str, float] = defaultdict(float)
        self.file_reasons: defaultdict[str, list[str]] = defaultdict(list)
        self.snippets: list[dict[str, Any]] = []
        self.spans: list[dict[str, Any]] = []
        self.path_name_hits: list[dict[str, Any]] = []
        self.flood_events: list[dict[str, Any]] = []
        self._seen_snippet_keys: set[tuple[str, str, str]] = set()
        self._seen_snippet_text: set[tuple[str, str]] = set()

    def add_file(self, path: str, score: float, reason: str) -> None:
        normalized = _norm_path(path)
        if not normalized:
            return
        self.file_scores[normalized] += score
        if reason not in self.file_reasons[normalized]:
            self.file_reasons[normalized].append(reason)

    def add_snippet(self, *, file_path: str, line: str, text: str, term: str, source: str) -> None:
        if len(self.snippets) >= self.limits["max_lines"]:
            self.flood_events.append(
                {
                    "kind": "max_lines_reached",
                    "limit": self.limits["max_lines"],
                    "label": "line_evidence_cap",
                }
            )
            return
        normalized = _norm_path(file_path)
        stripped = text.strip()
        key = (normalized, str(line), str(term).lower())
        text_key = (normalized, stripped)
        if key in self._seen_snippet_keys or text_key in self._seen_snippet_text:
            return
        bytes_to_add = len(stripped.encode("utf-8", errors="ignore")) + len(normalized.encode("utf-8", errors="ignore"))
        if self.context_bytes + bytes_to_add > self.limits["max_bytes"]:
            self.flood_events.append(
                {
                    "kind": "max_bytes_reached",
                    "limit": self.limits["max_bytes"],
                    "label": "context_payload_cap",
                }
            )
            return
        self._seen_snippet_keys.add(key)
        self._seen_snippet_text.add(text_key)
        snippet = {
            "file": normalized,
            "line": int(line) if str(line).isdigit() else line,
            "text": stripped,
            "query_term": term,
            "source": source,
            "evidence_type": "text_evidence",
            "evidence_role": "lexical/text/path evidence",
            "graph_proof": False,
            "proof_status": "not_graph_proof",
        }
        self.snippets.append(snippet)
        self.spans.append({"file": normalized, "line": snippet["line"], "query_term": term})
        self.context_bytes += bytes_to_add
        self.add_file(normalized, 120.0, f"line_evidence:{term}")


def _limits(task: dict, budget: ProviderBudget) -> dict[str, int]:
    max_commands = budget.max_tool_calls or int(task.get("max_tool_calls") or 12)
    return {
        "max_commands": max(1, int(max_commands)),
        "max_files": min(60, max(10, int(task.get("max_files") or 40))),
        "max_lines": min(120, max(10, int(task.get("max_lines") or 80))),
        "max_bytes": max(1000, int(budget.max_context_bytes or task.get("max_output_bytes") or 60000)),
        "per_command_timeout_s": max(1, min(20, int(budget.max_time_s or 10))),
        "per_query_result_cap": min(40, max(5, int(task.get("per_query_result_cap") or 20))),
        "per_file_line_cap": min(8, max(1, int(task.get("per_file_line_cap") or 2))),
    }


def _build_plan(rg: str, task: dict, limits: dict[str, int]) -> list[dict[str, Any]]:
    clues = _extract_clues(task)
    base_globs = _glob_args(EXCLUDE_GLOBS)
    plan: list[dict[str, Any]] = [
        {
            "command_id": "rg_planned_files",
            "kind": "file_discovery",
            "argv": [rg, "--no-ignore", "--files", *base_globs],
            "why": "Discover repository file names for path/name clues before content search.",
            "expected_signal": "file names, directory names, extensions, config filenames",
            "path_scope": "repo minus ignored generated/artifact paths",
            "path_filters": clues["path_filters"],
            "flood_risk": "medium",
            "query_term": "",
            "clues": clues,
        }
    ]
    selected_terms = clues["search_terms"][: max(1, limits["max_commands"] - 1)]
    for index, term_info in enumerate(selected_terms, 1):
        term = term_info["term"]
        scope_globs = _scope_globs(term, clues)
        common = [*base_globs, *_glob_args(scope_globs), "--", term, "."]
        plan.append(
            {
                "command_id": f"rg_planned_shortlist_{index}_{safe_slug(term)[:24]}",
                "kind": "file_shortlist",
                "argv": [rg, "--no-ignore", "-l", "--max-count", "1", *common],
                "why": f"Shortlist files containing visible clue `{term}`.",
                "expected_signal": "content match file shortlist",
                "path_scope": "repo minus ignored paths; " + _scope_description(scope_globs),
                "path_filters": scope_globs,
                "flood_risk": term_info["flood_risk"],
                "query_term": term,
                "query_source": term_info["source"],
            }
        )
        plan.append(
            {
                "command_id": f"rg_planned_lines_{index}_{safe_slug(term)[:24]}",
                "kind": "line_evidence",
                "argv": [
                    rg,
                    "--no-ignore",
                    "-n",
                    "-S",
                    "--max-count",
                    str(limits["per_file_line_cap"]),
                    *common,
                ],
                "why": f"Collect bounded line evidence for visible clue `{term}`.",
                "expected_signal": "line-numbered lexical evidence snippets",
                "path_scope": "repo minus ignored paths; " + _scope_description(scope_globs),
                "path_filters": scope_globs,
                "flood_risk": term_info["flood_risk"],
                "query_term": term,
                "query_source": term_info["source"],
            }
        )
        if len(plan) >= limits["max_commands"]:
            break
    return plan


def _extract_clues(task: dict) -> dict[str, Any]:
    text = _task_text(task)
    provenance = task.get("visible_query_term_sources", [])
    terms: list[dict[str, str]] = []
    for item in query_terms(task):
        terms.append({"term": item, "source": _source_for_term(item, provenance)})
    for token in _tokens(text):
        terms.append({"term": token, "source": "prompt"})
    for field, source in (
        ("visible_file_hints", "visible_file_hint"),
        ("visible_symbol_hints", "visible_symbol_hint"),
        ("visible_text_hints", "visible_text_hint"),
        ("visible_config_keys", "config_key"),
        ("visible_error_messages", "error_message"),
        ("visible_stack_frames", "stack_frame"),
    ):
        for value in _iter_values(task.get(field)):
            terms.append({"term": value, "source": source})
    if str(task.get("task_type") or "").lower() == "implementation_trace":
        for token in _tokens(text):
            terms.append({"term": token, "source": "implementation_trace"})
    search_terms = _dedupe_terms(terms)
    path_like_terms = [item["term"] for item in search_terms if _is_path_like(item["term"])]
    identifier_terms = [item["term"] for item in search_terms if _is_identifier_like(item["term"])]
    extension_hints = _extension_hints(text, search_terms, task)
    config_hints = [term for term in (item["term"] for item in search_terms) if Path(term.replace("\\", "/")).name in CONFIG_FILENAMES]
    for item in search_terms:
        item["flood_risk"] = _flood_risk(item["term"])
    return {
        "task_type": task.get("task_type"),
        "search_terms": search_terms,
        "path_like_terms": path_like_terms,
        "identifier_terms": identifier_terms,
        "extension_hints": extension_hints,
        "config_hints": config_hints,
        "path_filters": _path_filters(text, extension_hints, config_hints),
    }


def _consume_file_discovery(stdout: str, command: dict[str, Any], state: _ProviderState) -> None:
    lines = [line.strip() for line in stdout.splitlines() if line.strip()]
    state.files_seen += len(lines)
    clues = command.get("clues", {})
    if len(lines) > state.limits["max_files"] * 20:
        state.flood_events.append(
            {
                "kind": "file_discovery_large_repo",
                "files_seen": len(lines),
                "label": "file_discovery_scope",
            }
        )
    for path in lines:
        normalized = _norm_path(path)
        score, reasons = _path_score(normalized, clues)
        if score <= 0:
            continue
        state.add_file(normalized, score, ";".join(reasons))
        hit = {"file": normalized, "score": score, "reasons": reasons}
        if len(state.path_name_hits) < state.limits["max_files"]:
            state.path_name_hits.append(hit)
        bytes_to_add = len(normalized.encode("utf-8", errors="ignore"))
        if state.context_bytes + bytes_to_add <= state.limits["max_bytes"]:
            state.context_bytes += bytes_to_add


def _consume_file_shortlist(stdout: str, command: dict[str, Any], state: _ProviderState) -> None:
    lines = [_norm_path(line.strip()) for line in stdout.splitlines() if line.strip()]
    term = command.get("query_term", "")
    if len(lines) > state.limits["per_query_result_cap"]:
        state.flood_events.append(
            {
                "kind": "per_query_file_cap",
                "term": term,
                "seen": len(lines),
                "kept": state.limits["per_query_result_cap"],
                "label": "file_shortlist_cap",
            }
        )
    for path in lines[: state.limits["per_query_result_cap"]]:
        state.add_file(path, 180.0, f"file_shortlist:{term}")


def _consume_line_evidence(stdout: str, command: dict[str, Any], state: _ProviderState) -> None:
    lines = [line for line in stdout.splitlines() if line.strip()]
    term = command.get("query_term", "")
    state.lines_seen += len(lines)
    if len(lines) > state.limits["per_query_result_cap"]:
        state.flood_events.append(
            {
                "kind": "per_query_line_cap",
                "term": term,
                "seen": len(lines),
                "kept": state.limits["per_query_result_cap"],
                "label": "line_evidence_cap",
            }
        )
    for line in lines[: state.limits["per_query_result_cap"]]:
        parsed = _parse_rg_line(line)
        if parsed is None:
            continue
        file_path, line_number, text = parsed
        state.add_snippet(
            file_path=file_path,
            line=line_number,
            text=text,
            term=term,
            source=str(command.get("query_source") or "rg"),
        )


def _add_ranked_files(packet: ContextPacket, state: _ProviderState) -> None:
    ranked = _ranked_files(state)
    for path in ranked[: state.limits["max_files"]]:
        packet.files.append(path)


def _add_snippets(packet: ContextPacket, state: _ProviderState) -> None:
    packet.snippets.extend(state.snippets)
    packet.spans.extend(state.spans)


def _ranked_files(state: _ProviderState) -> list[str]:
    return [
        item[0]
        for item in sorted(
            state.file_scores.items(),
            key=lambda pair: (-pair[1], pair[0]),
        )
    ]


def _top_candidate_details(state: _ProviderState, limit: int) -> list[dict[str, Any]]:
    return [
        {
            "file": path,
            "score": score,
            "reasons": state.file_reasons.get(path, []),
        }
        for path, score in sorted(state.file_scores.items(), key=lambda pair: (-pair[1], pair[0]))[:limit]
    ]


def _path_score(path: str, clues: dict[str, Any]) -> tuple[float, list[str]]:
    lower_path = path.lower()
    name = Path(path).name.lower()
    stem = Path(path).stem.lower()
    score = 0.0
    reasons: list[str] = []
    for term in clues.get("search_terms", []):
        raw = str(term.get("term", ""))
        clean = raw.replace("\\", "/").strip()
        key = clean.lower()
        if not key:
            continue
        key_name = Path(key).name
        key_stem = Path(key).stem
        if lower_path == key or name == key:
            score += 1000.0
            reasons.append(f"exact_filename:{raw}")
        elif key_name and name == key_name:
            score += 850.0
            reasons.append(f"basename_match:{raw}")
        elif key_stem and stem == key_stem and "." not in key_stem:
            score += 450.0
            reasons.append(f"stem_match:{raw}")
        elif key in lower_path:
            score += 220.0 if _is_path_like(clean) else 80.0
            reasons.append(f"path_contains:{raw}")
        elif key.replace("-", "_") in lower_path or key.replace("_", "-") in lower_path:
            score += 90.0
            reasons.append(f"normalized_path_contains:{raw}")
    suffix = Path(path).suffix.lower()
    if suffix and suffix in clues.get("extension_hints", []):
        score += 35.0
        reasons.append(f"extension_hint:{suffix}")
    if name in {value.lower() for value in CONFIG_FILENAMES}:
        score += 60.0
        reasons.append("config_filename")
    if "/docs/" in f"/{lower_path}" or name.startswith("readme"):
        score += 20.0
        reasons.append("doc_path")
    return score, reasons


def _dedupe_terms(terms: list[dict[str, str]]) -> list[dict[str, str]]:
    result: list[dict[str, str]] = []
    seen: set[str] = set()
    priority = {
        "visible_seed": 0,
        "visible_file_hint": 0,
        "stack_frame": 0,
        "config_key": 1,
        "implementation_trace": 1,
        "visible_symbol_hint": 1,
        "prompt": 2,
        "visible_text_hint": 2,
        "error_message": 3,
    }
    for item in sorted(enumerate(terms), key=lambda pair: (priority.get(pair[1].get("source", ""), 5), pair[0])):
        item = item[1]
        clean = _clean_term(item.get("term", ""))
        if len(clean) < 2:
            continue
        if clean.lower() in STOPWORDS and item.get("source") not in {"visible_seed", "visible_file_hint", "stack_frame", "config_key"}:
            continue
        key = clean.lower().replace("\\", "/")
        if key in seen:
            continue
        seen.add(key)
        result.append({"term": clean, "source": item.get("source") or "prompt"})
        if len(result) >= 16:
            break
    return result


def _scope_globs(term: str, clues: dict[str, Any]) -> tuple[str, ...]:
    lower = term.lower()
    globs: list[str] = []
    if lower.endswith(".mk") or ".mk" in clues.get("extension_hints", []):
        globs.append("*.mk")
    if lower == "config.in" or "Config.in" in clues.get("config_hints", []):
        globs.append("**/Config.in")
    if lower.endswith(".adoc") or ".adoc" in clues.get("extension_hints", []):
        globs.append("*.adoc")
    if lower.endswith(".md") or ".md" in clues.get("extension_hints", []):
        globs.append("*.md")
    if lower.endswith(".rs") or ".rs" in clues.get("extension_hints", []):
        globs.append("*.rs")
    return tuple(dict.fromkeys(globs))


def _scope_description(globs: tuple[str, ...]) -> str:
    if not globs:
        return "no positive extension/path filter"
    return "positive filters " + ",".join(globs)


def _glob_args(globs: tuple[str, ...]) -> list[str]:
    args: list[str] = []
    for glob in globs:
        args.extend(["--glob", glob])
    return args


def _path_filters(text: str, extension_hints: list[str], config_hints: list[str]) -> list[str]:
    filters = list(extension_hints)
    filters.extend(config_hints)
    lower = text.lower()
    if "docs" in lower or "documentation" in lower:
        filters.append("docs/**")
    if "support script" in lower or "support scripts" in lower:
        filters.append("support/**")
    return list(dict.fromkeys(filters))


def _extension_hints(text: str, search_terms: list[dict[str, str]], task: dict) -> list[str]:
    lower = text.lower()
    hints: list[str] = []
    visible_language = str(task.get("visible_language") or "").lower()
    for ext in LANGUAGE_EXTENSIONS.get(visible_language, []):
        hints.append(ext)
    for language, extensions in LANGUAGE_EXTENSIONS.items():
        if language in lower:
            hints.extend(extensions)
    if ".mk" in lower or " makefile" in lower:
        hints.append(".mk")
    if "config.in" in lower:
        hints.append(".in")
    if "docs" in lower or "documentation" in lower or "manual" in lower:
        hints.extend([".md", ".adoc"])
    if "schema" in lower:
        hints.append(".json")
    for item in search_terms:
        suffix = Path(item["term"].replace("\\", "/")).suffix
        if suffix:
            hints.append(suffix.lower())
    return list(dict.fromkeys(hints))


def _source_for_term(term: str, provenance: Any) -> str:
    if isinstance(provenance, list):
        key = term.lower()
        for item in provenance:
            if isinstance(item, dict) and str(item.get("term", "")).lower() == key:
                return str(item.get("source") or "visible_seed")
    return "visible_seed"


def _task_text(task: dict) -> str:
    for key in ("task_text", "prompt", "task"):
        value = task.get(key)
        if isinstance(value, str) and value.strip():
            return value
    return ""


def _iter_values(value: Any) -> list[str]:
    if value is None:
        return []
    if isinstance(value, str):
        return [value]
    if isinstance(value, dict):
        return [str(item) for item in value.values() if str(item).strip()]
    if isinstance(value, list):
        return [str(item) for item in value if str(item).strip()]
    return [str(value)]


def _tokens(text: str) -> list[str]:
    return [
        token
        for token in re.findall(r"[A-Za-z_][A-Za-z0-9_]{2,}|[\w./\\-]+\.[A-Za-z0-9_]+", text)
        if len(token.strip(".,:;()[]{}\"'`")) > 2
    ]


def _is_path_like(term: str) -> bool:
    clean = term.replace("\\", "/")
    return "/" in clean or "." in Path(clean).name or Path(clean).name in CONFIG_FILENAMES


def _is_identifier_like(term: str) -> bool:
    return bool(re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", term))


def _flood_risk(term: str) -> str:
    clean = _clean_term(term)
    if len(clean) <= 3 or clean.lower() in {"find", "trace", "task", "file", "source", "docs", "path"}:
        return "high"
    if " " in clean:
        return "medium"
    return "low"


def _clean_term(term: str) -> str:
    return str(term).strip(" \t\r\n.,:;()[]{}\"'`")


def _parse_rg_line(line: str) -> tuple[str, str, str] | None:
    parts = line.split(":", 2)
    if len(parts) < 3:
        return None
    return _norm_path(parts[0]), parts[1], parts[2]


def _norm_path(path: str) -> str:
    return path.replace("\\", "/").lstrip("./")


def _read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return ""
