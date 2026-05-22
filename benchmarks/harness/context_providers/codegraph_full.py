from __future__ import annotations

from benchmarks.harness.context_providers.base import ContextPacket, ProviderBudget
from benchmarks.harness.context_providers.codegraph_exact_text import CodeGraphExactTextProvider


class CodeGraphFullProvider(CodeGraphExactTextProvider):
    mode = "codegraph_full"

    def get_context(self, task: dict, budget: ProviderBudget) -> ContextPacket:
        return self._run_codegraph(task, budget, full=True)

    def metadata(self) -> dict:
        return {
            "mode": self.mode,
            "uses_rg": False,
            "uses_codegraph": True,
            "uses_external_agent": False,
            "vector_candidates_enabled": True,
            "nuance_candidates_enabled": True,
        }
