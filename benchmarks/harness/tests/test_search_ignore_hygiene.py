from __future__ import annotations

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


class SearchIgnoreHygieneTests(unittest.TestCase):
    def test_broad_ripgrep_file_listing_excludes_generated_payloads(self) -> None:
        rg = Path(".codex-tools/rg.exe")
        rg_cmd = str(rg) if rg.exists() else shutil.which("rg")
        if not rg_cmd:
            self.skipTest("ripgrep is not available")

        proc = subprocess.run(
            [rg_cmd, "--files", "--no-ignore-parent"],
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        listed = proc.stdout.replace("\\", "/").splitlines()
        forbidden_prefixes = (
            "benchmarks/workspaces/",
            "benchmarks/results/",
            "reports/audit/artifacts/",
            "reports/final/full_benchmark_sweep_charts/",
        )
        forbidden_contains = (
            "/workspaces/",
            "/results/",
            "/upstream/",
        )
        offenders = [
            path
            for path in listed
            if path.startswith(forbidden_prefixes)
            or (
                path.startswith("benchmarks/tracks/")
                and any(part in path for part in forbidden_contains)
            )
        ]
        self.assertEqual(offenders[:20], [])

    def test_explicit_root_ripgrep_uses_repo_ignore_file(self) -> None:
        rg = Path(".codex-tools/rg.exe")
        rg_cmd = str(rg) if rg.exists() else shutil.which("rg")
        if not rg_cmd:
            self.skipTest("ripgrep is not available")

        proc = subprocess.run(
            [
                rg_cmd,
                "--files",
                "--no-ignore-parent",
                "--ignore-file",
                ".rgignore",
                "benchmarks",
                "docs",
                "reports",
                "crates",
                "scripts",
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        listed = proc.stdout.replace("\\", "/").splitlines()
        offenders = [
            path
            for path in listed
            if path.startswith(
                (
                    "benchmarks/workspaces/",
                    "benchmarks/results/",
                    "reports/audit/artifacts/",
                    "reports/final/full_benchmark_sweep_charts/",
                )
            )
            or (
                path.startswith("benchmarks/tracks/")
                and any(part in path for part in ("/workspaces/", "/results/", "/upstream/"))
            )
        ]
        self.assertEqual(offenders[:20], [])

    def test_benchmark_workspace_rg_can_bypass_parent_ignore_for_fixture_repo(self) -> None:
        rg = Path(".codex-tools/rg.exe")
        rg_cmd = str(rg) if rg.exists() else shutil.which("rg")
        if not rg_cmd:
            self.skipTest("ripgrep is not available")

        scratch_root = Path("benchmarks/results/summaries")
        scratch_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="rg_parent_ignore_probe_", dir=scratch_root) as temp_dir:
            fixture = Path(temp_dir)
            (fixture / "src").mkdir(parents=True)
            (fixture / "src" / "mini.py").write_text("class Printable: pass\n", encoding="utf-8")

            proc = subprocess.run(
                [rg_cmd, "--files", "--no-ignore-parent"],
                cwd=fixture,
                text=True,
                capture_output=True,
                check=False,
                timeout=30,
            )

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("src/mini.py", proc.stdout.replace("\\", "/").splitlines())


if __name__ == "__main__":
    unittest.main()
