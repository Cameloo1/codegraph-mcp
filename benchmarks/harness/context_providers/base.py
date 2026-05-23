from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class ProviderBudget:
    max_time_s: int | None = None
    max_tool_calls: int | None = None
    max_context_bytes: int = 60000


@dataclass
class ContextPacket:
    files: list[Any] = field(default_factory=list)
    symbols: list[Any] = field(default_factory=list)
    spans: list[Any] = field(default_factory=list)
    snippets: list[Any] = field(default_factory=list)
    proof_paths: list[Any] = field(default_factory=list)
    text_evidence: list[Any] = field(default_factory=list)
    unknowns: list[Any] = field(default_factory=list)
    risks: list[Any] = field(default_factory=list)
    validation_steps: list[Any] = field(default_factory=list)
    follow_up_queries: list[Any] = field(default_factory=list)
    claimability: dict[str, Any] = field(default_factory=dict)
    raw_context_bytes: int = 0
    tool_calls: int = 0
    raw: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "files": self.files,
            "symbols": self.symbols,
            "spans": self.spans,
            "snippets": self.snippets,
            "proof_paths": self.proof_paths,
            "text_evidence": self.text_evidence,
            "unknowns": self.unknowns,
            "risks": self.risks,
            "validation_steps": self.validation_steps,
            "follow_up_queries": self.follow_up_queries,
            "claimability": self.claimability,
            "raw_context_bytes": self.raw_context_bytes,
            "tool_calls": self.tool_calls,
            "raw": self.raw,
        }


class ContextProvider:
    mode = "baseline"

    def __init__(self, repo_root: Path, workspace: Path, release_binary: Path | None = None):
        self.repo_root = repo_root
        self.workspace = workspace
        self.release_binary = release_binary

    def prepare_repo(self, task: dict) -> dict[str, Any]:
        return {}

    def get_context(self, task: dict, budget: ProviderBudget) -> ContextPacket:
        raise NotImplementedError

    def cleanup(self) -> None:
        return None

    def metadata(self) -> dict[str, Any]:
        return {
            "mode": self.mode,
            "uses_rg": False,
            "uses_codegraph": False,
            "uses_external_agent": False,
        }


def query_terms(task: dict) -> list[str]:
    terms = task.get("visible_query_terms") or task.get("query_terms")
    if isinstance(terms, list) and terms:
        return [str(term) for term in terms]
    words = [part.strip(".,:;()[]{}\"'") for part in str(task.get("task_text") or task.get("prompt") or task.get("task", "")).split()]
    return [word for word in words if len(word) > 3][:5]
