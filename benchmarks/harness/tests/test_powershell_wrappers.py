import shutil
import subprocess
import tempfile
import unittest
import json
from pathlib import Path


class PowerShellWrapperTests(unittest.TestCase):
    def test_native_tool_wrappers_propagate_last_exit_code(self):
        scripts = [
            "benchmarks/scripts/run_swebench_harness_linux_container.ps1",
            "benchmarks/scripts/setup_crosscodeeval_docker.ps1",
            "benchmarks/scripts/setup_crosscodeeval_windows_msvc.ps1",
            "benchmarks/scripts/setup_crosscodeeval_wsl.ps1",
            "benchmarks/scripts/setup_repobench.ps1",
            "benchmarks/scripts/setup_swebench_wsl.ps1",
            "benchmarks/scripts/run_codex_external_patch_agent.ps1",
        ]
        for script in scripts:
            with self.subTest(script=script):
                text = Path(script).read_text(encoding="utf-8")
                if script.endswith("run_codex_external_patch_agent.ps1"):
                    self.assertIn("System.Diagnostics.ProcessStartInfo", text)
                    self.assertIn("exit $proc.ExitCode", text)
                else:
                    self.assertIn("$LASTEXITCODE", text)
                    self.assertIn("exit $LASTEXITCODE", text)

    def test_swebench_linux_container_wrapper_propagates_docker_failure(self):
        powershell = shutil.which("powershell") or shutil.which("pwsh")
        if powershell is None:
            self.skipTest("PowerShell is not available")
        with tempfile.TemporaryDirectory() as tmp:
            fake_docker = Path(tmp) / "fake_docker.cmd"
            fake_docker.write_text("@echo off\r\necho simulated docker failure 1>&2\r\nexit /b 42\r\n", encoding="utf-8")
            proc = subprocess.run(
                [
                    powershell,
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    "benchmarks/scripts/run_swebench_harness_linux_container.ps1",
                    "-DockerCommand",
                    str(fake_docker),
                ],
                text=True,
                capture_output=True,
                check=False,
                timeout=30,
            )
            self.assertEqual(proc.returncode, 42, proc.stderr)

    def test_codex_external_agent_wrapper_returns_git_diff_from_workspace(self):
        powershell = shutil.which("powershell") or shutil.which("pwsh")
        if powershell is None:
            self.skipTest("PowerShell is not available")
        if shutil.which("git") is None:
            self.skipTest("git is not available")

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init"], cwd=repo, check=True, capture_output=True, text=True)
            subprocess.run(["git", "config", "user.email", "bench@example.invalid"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Benchmark Test"], cwd=repo, check=True)
            (repo / "sample.py").write_text("value = 1\n", encoding="utf-8")
            subprocess.run(["git", "add", "sample.py"], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "seed"], cwd=repo, check=True, capture_output=True, text=True)
            (repo / "sample.py").write_text("value = 2\n", encoding="utf-8")

            fake_codex = root / "fake_codex.cmd"
            fake_codex.write_text("@echo off\r\nexit /b 0\r\n", encoding="ascii")
            payload = {
                "task": {
                    "task_id": "wrapper_diff_probe",
                    "workspace_path": str(repo),
                    "task": "probe",
                }
            }
            proc = subprocess.run(
                [
                    powershell,
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    "benchmarks/scripts/run_codex_external_patch_agent.ps1",
                    "-CodexCommand",
                    str(fake_codex),
                    "-TimeoutSeconds",
                    "5",
                    "-StationMode",
                    "never",
                ],
                input=json.dumps(payload),
                text=True,
                capture_output=True,
                check=False,
                timeout=30,
            )

            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertTrue(proc.stdout.lstrip().startswith("diff --git"), proc.stdout)

    def test_codex_external_agent_wrapper_station_dry_run_writes_launch_command(self):
        powershell = shutil.which("powershell") or shutil.which("pwsh")
        if powershell is None:
            self.skipTest("PowerShell is not available")
        if shutil.which("git") is None:
            self.skipTest("git is not available")

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init"], cwd=repo, check=True, capture_output=True, text=True)
            subprocess.run(["git", "config", "user.email", "bench@example.invalid"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Benchmark Test"], cwd=repo, check=True)
            (repo / "sample.py").write_text("value = 1\n", encoding="utf-8")
            subprocess.run(["git", "add", "sample.py"], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "seed"], cwd=repo, check=True, capture_output=True, text=True)
            (repo / "sample.py").write_text("value = 2\n", encoding="utf-8")

            fake_codex = root / "fake_codex.cmd"
            fake_codex.write_text("@echo off\r\nexit /b 0\r\n", encoding="ascii")
            task_id = "wrapper_station_probe"
            payload = {
                "task": {
                    "task_id": task_id,
                    "workspace_path": str(repo),
                    "task": "probe",
                },
                "context": {"provider": "test", "raw_context_bytes": 12, "tool_calls": 1},
            }
            proc = subprocess.run(
                [
                    powershell,
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    "benchmarks/scripts/run_codex_external_patch_agent.ps1",
                    "-CodexCommand",
                    str(fake_codex),
                    "-TimeoutSeconds",
                    "5",
                    "-StationMode",
                    "always",
                    "-StationDryRun",
                ],
                input=json.dumps(payload),
                text=True,
                capture_output=True,
                check=False,
                timeout=30,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            log_roots = sorted(Path("benchmarks/workspaces/external_patch_agent_logs").glob(f"{task_id}_*"))
            self.assertTrue(log_roots)
            launch_text = (log_roots[-1] / "station_launch_command.txt").read_text(encoding="utf-8")
            status = json.loads((log_roots[-1] / "agent_status.json").read_text(encoding="utf-8"))
            self.assertIn("show_external_agent_station.ps1", launch_text)
            self.assertEqual(status["state"], "completed")
            self.assertEqual(status["context_bytes"], 12)

    def test_external_agent_station_renders_synthetic_status_once(self):
        powershell = shutil.which("powershell") or shutil.which("pwsh")
        if powershell is None:
            self.skipTest("PowerShell is not available")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            status_path = root / "agent_status.json"
            status_path.write_text(
                json.dumps(
                    {
                        "state": "running",
                        "task_id": "station_probe",
                        "workspace": str(root),
                        "log_root": str(root),
                        "timeout_seconds": 5,
                        "codex_pid": None,
                        "message": "synthetic",
                        "context_bytes": 42,
                        "tool_calls": 2,
                        "mode": "test",
                        "started_at_epoch": 0,
                    }
                ),
                encoding="utf-8",
            )
            (root / "codex_stdout.jsonl").write_text('{"event":"ok"}\n', encoding="utf-8")
            (root / "codex_stderr.txt").write_text("stderr\n", encoding="utf-8")
            (root / "codex_last_message.txt").write_text("last\n", encoding="utf-8")
            proc = subprocess.run(
                [
                    powershell,
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    "benchmarks/scripts/show_external_agent_station.ps1",
                    "-LogRoot",
                    str(root),
                    "-TaskId",
                    "station_probe",
                    "-Workspace",
                    str(root),
                    "-TimeoutSeconds",
                    "5",
                    "-StatusPath",
                    str(status_path),
                    "-Once",
                ],
                text=True,
                capture_output=True,
                check=False,
                timeout=15,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn("CodeGraph External Agent Station", proc.stdout)


if __name__ == "__main__":
    unittest.main()
