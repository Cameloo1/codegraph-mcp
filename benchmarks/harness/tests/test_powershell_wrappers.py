import shutil
import subprocess
import tempfile
import unittest
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


if __name__ == "__main__":
    unittest.main()
