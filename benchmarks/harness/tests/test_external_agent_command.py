import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmarks.harness.agent_scaffolds.external_agent_command import (
    ExternalAgentCommand,
    _command_from_env,
)
from benchmarks.harness.runners.run_patch_eval import validate_external_agent_command
from benchmarks.harness.runners.run_patch_eval import resolve_external_agent_command


class ExternalAgentCommandTests(unittest.TestCase):
    def test_prefers_canonical_external_agent_env_var(self):
        with patch.dict(
            os.environ,
            {
                "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND": "powershell -File benchmarks/tracks/swebench_lite/scripts/run_codex_external_patch_agent.ps1",
                "CODEGRAPH_BENCH_AGENT_COMMAND": "legacy-command",
            },
            clear=False,
        ):
            self.assertEqual(
                _command_from_env(),
                ["powershell", "-File", "benchmarks/tracks/swebench_lite/scripts/run_codex_external_patch_agent.ps1"],
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

    def test_bad_external_agent_executable_reports_exact_blocker(self):
        result = validate_external_agent_command(
            "definitely_missing_codegraph_external_agent_command --probe",
            "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND",
            {},
        )
        self.assertEqual(result["status"], "blocked_command_not_found")
        self.assertTrue(result["configured"])
        self.assertIn("definitely_missing_codegraph_external_agent_command", result["blocker"])

    def test_external_agent_command_resolution_prefers_cli_then_env_then_config(self):
        agent_cfg = {
            "external_agent_command_env": "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND",
            "external_agent_command": "config-command",
        }
        with patch.dict(os.environ, {"CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND": "env-command"}, clear=True):
            self.assertEqual(
                resolve_external_agent_command(agent_cfg, explicit_command="cli-command"),
                ("cli-command", "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND", "explicit_cli"),
            )
            self.assertEqual(
                resolve_external_agent_command(agent_cfg),
                ("env-command", "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND", "env"),
            )
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(
                resolve_external_agent_command(agent_cfg),
                ("config-command", "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND", "config"),
            )
            self.assertEqual(
                resolve_external_agent_command({"external_agent_command_env": "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND"}),
                ("", "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND", "missing"),
            )

    def test_external_agent_validation_does_not_log_secret_values(self):
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / "fake_external_agent"
            marker.write_text("placeholder\n", encoding="utf-8")
            with patch.dict(os.environ, {"OPENAI_API_KEY": "sk-should-not-be-logged"}, clear=False):
                result = validate_external_agent_command(
                    str(marker),
                    "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND",
                    {"required_env": ["OPENAI_API_KEY"]},
                )
        self.assertEqual(result["status"], "ready")
        self.assertIn("OPENAI_API_KEY", result["required_env"])
        self.assertNotIn("sk-should-not-be-logged", repr(result))


if __name__ == "__main__":
    unittest.main()
