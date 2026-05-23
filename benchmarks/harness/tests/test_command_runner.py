import json
import sys
import tempfile
import unittest
from pathlib import Path

from benchmarks.harness.command_runner import CommandRunner
from benchmarks.harness.resource_guard import ResourceLimits


class CommandRunnerTests(unittest.TestCase):
    def test_success_command_is_logged(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp)
            record = runner.run([sys.executable, "-c", "print('ok')"], command_id="success")
            self.assertTrue(record.success)
            self.assertEqual(record.exit_code, 0)
            self.assertEqual(record.argv, [sys.executable, "-c", "print('ok')"])
            self.assertIn("ok", Path(record.stdout_path).read_text(encoding="utf-8"))

    def test_failing_command_is_classified_nonzero(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp)
            record = runner.run([sys.executable, "-c", "import sys; sys.exit(7)"], command_id="fail")
            self.assertFalse(record.success)
            self.assertEqual(record.exit_code, 7)
            self.assertEqual(record.failure_kind, "nonzero_exit")

    def test_timeout_command_is_classified_timeout(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp)
            record = runner.run(
                [sys.executable, "-c", "import time; time.sleep(5)"],
                command_id="timeout",
                timeout_s=1,
            )
            self.assertFalse(record.success)
            self.assertEqual(record.failure_kind, "timeout")

    def test_stdout_cap_stops_unbounded_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp)
            record = runner.run(
                [sys.executable, "-c", "import sys; sys.stdout.write('x' * 2000000); sys.stdout.flush()"],
                command_id="output-cap",
                max_stdout_bytes=4096,
            )
            self.assertFalse(record.success)
            self.assertEqual(record.failure_kind, "output_limit")
            self.assertLessEqual(record.stdout_bytes, 4096)

    def test_missing_command_is_classified_missing_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp)
            record = runner.run(["definitely_missing_codegraph_benchmark_command.exe"], command_id="missing")
            self.assertFalse(record.success)
            self.assertEqual(record.failure_kind, "missing_binary")

    def test_stdout_stderr_capture(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp)
            record = runner.run(
                [
                    sys.executable,
                    "-c",
                    "import sys; sys.stdout.write('out'); sys.stderr.write('err')",
                ],
                command_id="capture",
            )
            self.assertEqual(Path(record.stdout_path).read_text(encoding="utf-8"), "out")
            self.assertEqual(Path(record.stderr_path).read_text(encoding="utf-8"), "err")
            self.assertEqual(record.stdout_bytes, 3)
            self.assertEqual(record.stderr_bytes, 3)

    def test_env_allowlist_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp, env_allowlist=("VISIBLE_ENV",))
            record = runner.run(
                [sys.executable, "-c", "print('env')"],
                command_id="env",
                env={"VISIBLE_ENV": "yes", "SECRET_ENV": "no"},
            )
            self.assertEqual(record.env, {"VISIBLE_ENV": "yes"})

    def test_shell_string_is_parse_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp)
            record = runner.run("echo should-not-shell-parse", command_id="parse")
            self.assertFalse(record.success)
            self.assertEqual(record.failure_kind, "parse_error")
            self.assertEqual(record.argv, [])

    def test_commands_jsonl_is_parseable(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(tmp)
            runner.run([sys.executable, "-c", "print('one')"], command_id="one")
            runner.run([sys.executable, "-c", "print('two')"], command_id="two")
            lines = (Path(tmp) / "commands.jsonl").read_text(encoding="utf-8").splitlines()
            parsed = [json.loads(line) for line in lines]
            self.assertEqual([item["command_id"] for item in parsed], ["one", "two"])

    def test_resource_guard_logs_tiny_command_measurements(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(
                tmp,
                resource_limits=ResourceLimits(
                    enabled=True,
                    max_process_tree_rss_mib=512,
                    min_system_available_mib=0,
                    sample_interval_s=0.1,
                    violation_grace_samples=1,
                ),
            )
            record = runner.run([sys.executable, "-c", "print('ok')"], command_id="guard-ok", timeout_s=10)
            self.assertTrue(record.success, record.exception)
            self.assertIsInstance(record.resource_guard, dict)
            self.assertTrue(record.resource_guard["enabled"])

    def test_resource_guard_fails_memory_heavy_child(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(
                tmp,
                resource_limits=ResourceLimits(
                    enabled=True,
                    max_process_tree_rss_mib=8,
                    min_system_available_mib=0,
                    sample_interval_s=0.1,
                    violation_grace_samples=1,
                ),
            )
            record = runner.run(
                [sys.executable, "-c", "import time; x=bytearray(96*1024*1024); time.sleep(30)"],
                command_id="guard-memory",
                timeout_s=30,
            )
            self.assertFalse(record.success)
            self.assertEqual(record.failure_kind, "resource_limit")
            self.assertIsInstance(record.resource_guard, dict)
            self.assertIn("process_tree_rss_mib", record.resource_guard["violation_reason"])
            self.assertGreaterEqual(record.resource_guard["max_observed_process_tree_rss_mib"], 8)

    def test_resource_guard_accounts_for_process_tree_children(self):
        with tempfile.TemporaryDirectory() as tmp:
            runner = CommandRunner(
                tmp,
                resource_limits=ResourceLimits(
                    enabled=True,
                    max_process_tree_rss_mib=16,
                    min_system_available_mib=0,
                    sample_interval_s=0.1,
                    violation_grace_samples=1,
                ),
            )
            child_code = "import time; x=bytearray(96*1024*1024); time.sleep(30)"
            parent_code = (
                "import subprocess, sys, time; "
                "p=subprocess.Popen([sys.executable, '-c', %r]); "
                "time.sleep(30)"
            ) % child_code
            record = runner.run([sys.executable, "-c", parent_code], command_id="guard-tree", timeout_s=30)
            self.assertFalse(record.success)
            self.assertEqual(record.failure_kind, "resource_limit")
            self.assertGreaterEqual(record.resource_guard["max_observed_process_tree_rss_mib"], 16)


if __name__ == "__main__":
    unittest.main()
