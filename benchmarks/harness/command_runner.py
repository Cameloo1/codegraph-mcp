from __future__ import annotations

import json
import os
import subprocess
import threading
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Mapping, Sequence

from benchmarks.harness.resource_guard import (
    ResourceLimits,
    resource_limits_from_env,
    run_command_with_resource_guard,
)
from benchmarks.harness.workspace import ensure_dir, safe_slug


FAILURE_KINDS = {
    "timeout",
    "nonzero_exit",
    "missing_binary",
    "parse_error",
    "setup_blocked",
    "docker_unavailable",
    "external_agent_missing",
    "output_limit",
    "resource_limit",
    "unknown",
}

DEFAULT_ENV_ALLOWLIST = (
    "CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND",
    "CODEGRAPH_BENCH_AGENT_STATION",
    "CODEGRAPH_BENCH_MAX_PROCESS_TREE_RSS_MIB",
    "CODEGRAPH_BENCH_MIN_SYSTEM_AVAILABLE_MIB",
    "CODEGRAPH_BENCH_RESOURCE_GUARD",
    "CODEGRAPH_BENCH_RESOURCE_SAMPLE_INTERVAL_S",
    "CODEGRAPH_BENCH_RESOURCE_VIOLATION_GRACE_SAMPLES",
    "DOCKER_CONFIG",
    "DOCKER_HOST",
    "PYTHONPATH",
)


@dataclass(frozen=True)
class CommandLogRecord:
    command_id: str
    argv: list[str]
    cwd: str
    env: dict[str, str]
    start_time: str
    end_time: str
    wall_time_ms: int
    timeout_s: int | None
    exit_code: int | None
    success: bool
    stdout_path: str
    stderr_path: str
    stdout_bytes: int
    stderr_bytes: int
    exception: str | None
    failure_kind: str | None
    resource_guard: dict | None

    def to_dict(self) -> dict:
        return asdict(self)


class CommandRunner:
    """Run exact argv commands and persist durable JSONL/stdout/stderr logs."""

    def __init__(
        self,
        log_dir: str | Path,
        *,
        commands_jsonl: str | Path | None = None,
        env_allowlist: Sequence[str] = DEFAULT_ENV_ALLOWLIST,
        resource_limits: ResourceLimits | None = None,
    ):
        self.log_dir = ensure_dir(Path(log_dir))
        self.stdout_dir = ensure_dir(self.log_dir / "stdout")
        self.stderr_dir = ensure_dir(self.log_dir / "stderr")
        self.commands_jsonl = Path(commands_jsonl) if commands_jsonl else self.log_dir / "commands.jsonl"
        self.commands_jsonl.parent.mkdir(parents=True, exist_ok=True)
        self.env_allowlist = tuple(env_allowlist)
        self.resource_limits = resource_limits or resource_limits_from_env()
        self._counter = 0

    def run(
        self,
        argv: Sequence[str] | object,
        *,
        cwd: str | Path | None = None,
        timeout_s: int | None = None,
        env: Mapping[str, str] | None = None,
        env_allowlist: Sequence[str] | None = None,
        command_id: str | None = None,
        failure_kind_hint: str | None = None,
        max_stdout_bytes: int | None = None,
        max_stderr_bytes: int | None = None,
    ) -> CommandLogRecord:
        self._counter += 1
        argv_error = _validate_argv(argv)
        argv_list = list(argv) if argv_error is None else []
        command_id = command_id or self._command_id(argv_list)
        cwd_path = Path(cwd) if cwd is not None else Path.cwd()
        stdout_path = self.stdout_dir / f"{safe_slug(command_id)}.stdout.txt"
        stderr_path = self.stderr_dir / f"{safe_slug(command_id)}.stderr.txt"
        start_wall = time.perf_counter()
        start_time = _now_iso()
        exception: str | None = None
        exit_code: int | None = None
        stdout = b""
        stderr = b""
        failure_kind: str | None = None
        resource_guard: dict | None = None

        if argv_error is not None:
            exception = argv_error
            failure_kind = "parse_error"
        else:
            command_env = os.environ.copy()
            if env:
                command_env.update({str(key): str(value) for key, value in env.items()})
            try:
                if self.resource_limits.enabled or max_stdout_bytes is not None or max_stderr_bytes is not None:
                    guarded = run_command_with_resource_guard(
                        argv_list,
                        cwd=cwd_path,
                        env=command_env,
                        timeout_s=timeout_s,
                        resource_limits=self.resource_limits,
                        max_stdout_bytes=max_stdout_bytes,
                        max_stderr_bytes=max_stderr_bytes,
                    )
                    exit_code = guarded.exit_code
                    stdout = guarded.stdout
                    stderr = guarded.stderr
                    exception = guarded.exception
                    failure_kind = guarded.failure_kind
                    resource_guard = guarded.resource_guard
                else:
                    proc = subprocess.run(
                        argv_list,
                        cwd=str(cwd_path),
                        env=command_env,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        timeout=timeout_s,
                        check=False,
                    )
                    exit_code = proc.returncode
                    stdout = proc.stdout or b""
                    stderr = proc.stderr or b""
                    resource_guard = {"enabled": False}
            except subprocess.TimeoutExpired as exc:
                stdout = _bytes_or_empty(exc.stdout)
                stderr = _bytes_or_empty(exc.stderr)
                exception = f"TimeoutExpired: command exceeded {timeout_s}s"
                failure_kind = "timeout"
            except FileNotFoundError as exc:
                exception = f"FileNotFoundError: {exc}"
                failure_kind = "missing_binary"
            except Exception as exc:  # runner logs unexpected launch errors
                exception = f"{type(exc).__name__}: {exc}"
                failure_kind = "unknown"

        end_time = _now_iso()
        wall_ms = int((time.perf_counter() - start_wall) * 1000)
        success = exit_code == 0 and exception is None
        if not success and failure_kind is None:
            failure_kind = _classify_failure(argv_list, exit_code, stdout, stderr, failure_kind_hint)
        if failure_kind_hint in FAILURE_KINDS and failure_kind not in {"timeout", "missing_binary", "parse_error"}:
            failure_kind = failure_kind_hint

        stdout_path.write_bytes(stdout)
        stderr_path.write_bytes(stderr)
        record_env = dict(os.environ)
        if env:
            record_env.update({str(key): str(value) for key, value in env.items()})
        command_env_for_record = _allowlisted_env(record_env, env_allowlist or self.env_allowlist)
        record = CommandLogRecord(
            command_id=command_id,
            argv=argv_list,
            cwd=str(cwd_path),
            env=command_env_for_record,
            start_time=start_time,
            end_time=end_time,
            wall_time_ms=wall_ms,
            timeout_s=timeout_s,
            exit_code=exit_code,
            success=success,
            stdout_path=str(stdout_path),
            stderr_path=str(stderr_path),
            stdout_bytes=len(stdout),
            stderr_bytes=len(stderr),
            exception=exception,
            failure_kind=None if success else failure_kind or "unknown",
            resource_guard=resource_guard,
        )
        self._append(record)
        return record

    def _command_id(self, argv: Sequence[str]) -> str:
        stem = safe_slug(Path(argv[0]).name if argv else "invalid-command")
        return f"{self._counter:04d}_{stem}"

    def _append(self, record: CommandLogRecord) -> None:
        with self.commands_jsonl.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(record.to_dict(), sort_keys=True) + "\n")


def _validate_argv(argv: Sequence[str] | object) -> str | None:
    if isinstance(argv, (str, bytes)):
        return "argv must be a sequence of strings, not a shell command string"
    if not isinstance(argv, Sequence):
        return "argv must be a sequence of strings"
    if not argv:
        return "argv must not be empty"
    for item in argv:
        if not isinstance(item, str) or item == "":
            return "argv entries must be non-empty strings"
    return None


def _classify_failure(
    argv: Sequence[str],
    exit_code: int | None,
    stdout: bytes,
    stderr: bytes,
    hint: str | None,
) -> str:
    if hint in FAILURE_KINDS:
        return hint
    text = (stdout + b"\n" + stderr).decode("utf-8", errors="ignore").lower()
    command_text = " ".join(argv).lower()
    if "codegraph_bench_external_agent_command" in text or "external agent command" in text:
        return "external_agent_missing"
    if "docker" in command_text and exit_code not in (0, None):
        return "docker_unavailable"
    if "docker" in text and any(marker in text for marker in ("error", "denied", "not found", "cannot connect")):
        return "docker_unavailable"
    if "blocked" in text or "not configured" in text:
        return "setup_blocked"
    if exit_code not in (0, None):
        return "nonzero_exit"
    return "unknown"


def _run_limited_output(
    argv: Sequence[str],
    *,
    cwd_path: Path,
    env: Mapping[str, str],
    timeout_s: int | None,
    max_stdout_bytes: int | None,
    max_stderr_bytes: int | None,
) -> tuple[int | None, bytes, bytes, str | None]:
    proc = subprocess.Popen(
        list(argv),
        cwd=str(cwd_path),
        env=dict(env),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    stdout_chunks: list[bytes] = []
    stderr_chunks: list[bytes] = []
    state: dict[str, str | None] = {"limit_stream": None}

    def read_limited(pipe, chunks: list[bytes], limit: int | None, stream_name: str) -> None:
        total = 0
        try:
            while True:
                chunk = pipe.read(65536)
                if not chunk:
                    break
                if limit is None or total < limit:
                    remaining = None if limit is None else max(0, limit - total)
                    chunks.append(chunk if remaining is None else chunk[:remaining])
                total += len(chunk)
                if limit is not None and total > limit and state["limit_stream"] is None:
                    state["limit_stream"] = stream_name
                    proc.kill()
                    break
        finally:
            try:
                pipe.close()
            except Exception:
                pass

    threads = [
        threading.Thread(target=read_limited, args=(proc.stdout, stdout_chunks, max_stdout_bytes, "stdout"), daemon=True),
        threading.Thread(target=read_limited, args=(proc.stderr, stderr_chunks, max_stderr_bytes, "stderr"), daemon=True),
    ]
    for thread in threads:
        thread.start()
    try:
        exit_code = proc.wait(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        proc.kill()
        for thread in threads:
            thread.join(timeout=1)
        raise
    for thread in threads:
        thread.join(timeout=1)
    if state["limit_stream"] is not None and exit_code is None:
        exit_code = -2
    return exit_code, b"".join(stdout_chunks), b"".join(stderr_chunks), state["limit_stream"]


def _allowlisted_env(env: Mapping[str, str], allowlist: Sequence[str]) -> dict[str, str]:
    allowed = set(allowlist)
    return {key: str(value) for key, value in env.items() if key in allowed}


def _bytes_or_empty(value: bytes | str | None) -> bytes:
    if value is None:
        return b""
    if isinstance(value, bytes):
        return value
    return value.encode("utf-8", errors="replace")


def _now_iso() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%S%z")
