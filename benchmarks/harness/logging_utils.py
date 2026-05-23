from __future__ import annotations

import json
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path


@dataclass
class CommandRecord:
    command: list[str]
    cwd: str
    exit_code: int
    stdout: str
    stderr: str
    wall_time_ms: int
    log_path: Path | None = None


def run_command(command: list[str], cwd: Path, log_path: Path | None = None, timeout_s: int | None = None) -> CommandRecord:
    start = time.perf_counter()
    try:
        proc = subprocess.run(
            command,
            cwd=str(cwd),
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=True,
            timeout=timeout_s,
            check=False,
        )
        exit_code = proc.returncode
        stdout = proc.stdout
        stderr = proc.stderr
    except subprocess.TimeoutExpired as exc:
        exit_code = -1
        stdout = exc.stdout or ""
        stderr = (exc.stderr or "") + f"\nTIMEOUT after {timeout_s}s"
    wall = int((time.perf_counter() - start) * 1000)
    record = CommandRecord(command, str(cwd), exit_code, stdout, stderr, wall, log_path)
    if log_path:
        log_path.parent.mkdir(parents=True, exist_ok=True)
        log_path.write_text(
            json.dumps(
                {
                    "command": command,
                    "cwd": str(cwd),
                    "exit_code": exit_code,
                    "wall_time_ms": wall,
                    "stdout": stdout,
                    "stderr": stderr,
                },
                indent=2,
            ),
            encoding="utf-8",
        )
    return record
