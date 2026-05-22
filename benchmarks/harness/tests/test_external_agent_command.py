import os
import unittest
from unittest.mock import patch

from benchmarks.harness.agent_scaffolds.external_agent_command import (
    ExternalAgentCommand,
    _command_from_env,
)


class ExternalAgentCommandTests(unittest.TestCase):
    def test_prefers_canonical_external_agent_env_var(self):
        with patch.dict(
            os.environ,
            {
                "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND": "powershell -File benchmarks/scripts/run_codex_external_patch_agent.ps1",
                "CODEGRAPH_BENCH_AGENT_COMMAND": "legacy-command",
            },
            clear=False,
        ):
            self.assertEqual(
                _command_from_env(),
                ["powershell", "-File", "benchmarks/scripts/run_codex_external_patch_agent.ps1"],
            )

    def test_legacy_external_agent_env_var_still_works_as_fallback(self):
        with patch.dict(
            os.environ,
            {
                "CODEGRAPH_BENCH_AGENT_COMMAND": "legacy-command --flag",
            },
            clear=False,
        ):
            os.environ.pop("CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND", None)
            self.assertEqual(_command_from_env(), ["legacy-command", "--flag"])

    def test_missing_external_agent_command_is_blocked_not_configured(self):
        with patch.dict(os.environ, {}, clear=True):
            result = ExternalAgentCommand(command=None).run({"task_id": "t"}, {"files": []})
            self.assertEqual(result.status, "blocked_not_configured")
            self.assertFalse(result.quality_claim)


if __name__ == "__main__":
    unittest.main()
