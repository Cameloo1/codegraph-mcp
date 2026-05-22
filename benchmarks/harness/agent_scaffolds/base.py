from __future__ import annotations

from dataclasses import dataclass


@dataclass
class AgentResult:
    status: str
    patch: str | None = None
    raw_output: str = ""
    quality_claim: bool = False


class AgentScaffold:
    name = "base"

    def run(self, task: dict, context: dict) -> AgentResult:
        raise NotImplementedError

