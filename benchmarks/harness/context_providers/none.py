from __future__ import annotations

from benchmarks.harness.context_providers.base import ContextPacket, ContextProvider, ProviderBudget


class NoneProvider(ContextProvider):
    mode = "none"

    def get_context(self, task: dict, budget: ProviderBudget) -> ContextPacket:
        return ContextPacket(
            unknowns=["no context provider enabled"],
            claimability={"graph_proof": False, "source_navigation_only": False},
        )

