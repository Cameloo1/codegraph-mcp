from __future__ import annotations

from benchmarks.harness.agent_scaffolds.base import AgentResult, AgentScaffold


class MockAgent(AgentScaffold):
    name = "mock_agent"

    def run(self, task: dict, context: dict) -> AgentResult:
        return AgentResult(
            status="mock_non_quality",
            patch=None,
            raw_output="mock agent did not edit code",
            quality_claim=False,
        )

