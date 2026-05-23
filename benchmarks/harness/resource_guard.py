from __future__ import annotations

import argparse
import os
import signal
import subprocess
import sys
import threading
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Mapping, Sequence


DEFAULT_MAX_PROCESS_TREE_RSS_MIB = 12_288
DEFAULT_MIN_SYSTEM_AVAILABLE_MIB = 3_072
DEFAULT_SAMPLE_INTERVAL_S = 1.0
DEFAULT_VIOLATION_GRACE_SAMPLES = 3


@dataclass(frozen=True)
class ResourceLimits:
    enabled: bool = True
    max_process_tree_rss_mib: int = DEFAULT_MAX_PROCESS_TREE_RSS_MIB
    min_system_available_mib: int = DEFAULT_MIN_SYSTEM_AVAILABLE_MIB
    sample_interval_s: float = DEFAULT_SAMPLE_INTERVAL_S
    violation_grace_samples: int = DEFAULT_VIOLATION_GRACE_SAMPLES

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class GuardedCommandResult:
    exit_code: int | None
    stdout: bytes
    stderr: bytes
    exception: str | None
    failure_kind: str | None
    resource_guard: dict


def add_resource_guard_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--max-process-tree-rss-mib", type=int, default=None)
    parser.add_argument("--min-system-available-mib", type=int, default=None)
    parser.add_argument("--resource-sample-interval-s", type=float, default=None)
    parser.add_argument("--no-resource-guard", action="store_true")


def resource_limits_from_args(args: argparse.Namespace) -> ResourceLimits:
    base = resource_limits_from_env()
    return ResourceLimits(
        enabled=False if getattr(args, "no_resource_guard", False) else base.enabled,
        max_process_tree_rss_mib=(
            getattr(args, "max_process_tree_rss_mib", None) or base.max_process_tree_rss_mib
        ),
        min_system_available_mib=(
            getattr(args, "min_system_available_mib", None) or base.min_system_available_mib
        ),
        sample_interval_s=(
            getattr(args, "resource_sample_interval_s", None) or base.sample_interval_s
        ),
        violation_grace_samples=base.violation_grace_samples,
    )


def resource_limits_from_env() -> ResourceLimits:
    return ResourceLimits(
        enabled=_env_bool("CODEGRAPH_BENCH_RESOURCE_GUARD", default=True),
        max_process_tree_rss_mib=_env_int(
            "CODEGRAPH_BENCH_MAX_PROCESS_TREE_RSS_MIB",
            DEFAULT_MAX_PROCESS_TREE_RSS_MIB,
        ),
        min_system_available_mib=_env_int(
            "CODEGRAPH_BENCH_MIN_SYSTEM_AVAILABLE_MIB",
            DEFAULT_MIN_SYSTEM_AVAILABLE_MIB,
        ),
        sample_interval_s=_env_float(
            "CODEGRAPH_BENCH_RESOURCE_SAMPLE_INTERVAL_S",
            DEFAULT_SAMPLE_INTERVAL_S,
        ),
        violation_grace_samples=_env_int(
            "CODEGRAPH_BENCH_RESOURCE_VIOLATION_GRACE_SAMPLES",
            DEFAULT_VIOLATION_GRACE_SAMPLES,
        ),
    )


def summarize_resource_limit_failures(commands: Sequence[Mapping[str, object]]) -> dict:
    failures = [
        {
            "command_id": command.get("command_id"),
            "argv": command.get("argv"),
            "cwd": command.get("cwd"),
            "wall_time_ms": command.get("wall_time_ms"),
            "resource_guard": command.get("resource_guard"),
        }
        for command in commands
        if command.get("failure_kind") == "resource_limit"
    ]
    return {"count": len(failures), "commands": failures}


def run_command_with_resource_guard(
    argv: Sequence[str],
    *,
    cwd: str | Path,
    env: Mapping[str, str] | None = None,
    timeout_s: int | None = None,
    resource_limits: ResourceLimits | None = None,
    input_text: str | None = None,
    max_stdout_bytes: int | None = None,
    max_stderr_bytes: int | None = None,
) -> GuardedCommandResult:
    limits = resource_limits or resource_limits_from_env()
    guard_state = _new_guard_state(limits)
    command_env = dict(os.environ)
    if env:
        command_env.update({str(key): str(value) for key, value in env.items()})

    popen_kwargs: dict[str, object] = {
        "cwd": str(cwd),
        "env": command_env,
        "stdout": subprocess.PIPE,
        "stderr": subprocess.PIPE,
    }
    if input_text is not None:
        popen_kwargs["stdin"] = subprocess.PIPE
    if os.name != "nt":
        popen_kwargs["start_new_session"] = True

    proc = subprocess.Popen(list(argv), **popen_kwargs)
    stdout_chunks: list[bytes] = []
    stderr_chunks: list[bytes] = []
    state: dict[str, object] = {
        "limit_stream": None,
        "killed": False,
        "killed_pids": [],
    }

    if input_text is not None and proc.stdin is not None:
        try:
            proc.stdin.write(input_text.encode("utf-8", errors="replace"))
            proc.stdin.close()
        except BrokenPipeError:
            pass

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
                    state["killed_pids"] = _kill_process_tree(proc)
                    state["killed"] = True
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

    started = time.perf_counter()
    next_sample = started
    consecutive_violations = 0
    exception: str | None = None
    failure_kind: str | None = None
    violation_reason: str | None = None

    while proc.poll() is None:
        now = time.perf_counter()
        if timeout_s is not None and now - started > timeout_s:
            state["killed_pids"] = _kill_process_tree(proc)
            state["killed"] = True
            exception = f"TimeoutExpired: command exceeded {timeout_s}s"
            failure_kind = "timeout"
            break
        if state["limit_stream"] is not None:
            exception = f"OutputLimitExceeded: {state['limit_stream']} exceeded configured byte cap"
            failure_kind = "output_limit"
            break
        if limits.enabled and now >= next_sample:
            snapshot = _sample_resources(proc.pid)
            _record_snapshot(guard_state, snapshot)
            current_reason = _resource_violation_reason(limits, snapshot)
            if current_reason:
                consecutive_violations += 1
                violation_reason = current_reason
            else:
                consecutive_violations = 0
            if consecutive_violations >= max(1, limits.violation_grace_samples):
                state["killed_pids"] = _kill_process_tree(proc)
                state["killed"] = True
                exception = f"ResourceLimitExceeded: {violation_reason}"
                failure_kind = "resource_limit"
                break
            next_sample = now + max(0.1, limits.sample_interval_s)
        time.sleep(0.05)

    if state["killed"]:
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                proc.kill()
            except Exception:
                pass
            proc.wait(timeout=5)
    else:
        proc.wait()

    for thread in threads:
        thread.join(timeout=2)

    if state["limit_stream"] is not None and failure_kind is None:
        exception = f"OutputLimitExceeded: {state['limit_stream']} exceeded configured byte cap"
        failure_kind = "output_limit"

    guard_state["violation_reason"] = violation_reason if failure_kind == "resource_limit" else None
    guard_state["killed_process_ids"] = sorted(set(int(pid) for pid in state["killed_pids"]))
    return GuardedCommandResult(
        exit_code=proc.returncode,
        stdout=b"".join(stdout_chunks),
        stderr=b"".join(stderr_chunks),
        exception=exception,
        failure_kind=failure_kind,
        resource_guard=guard_state,
    )


def _new_guard_state(limits: ResourceLimits) -> dict:
    return {
        "enabled": limits.enabled,
        "max_process_tree_rss_mib": limits.max_process_tree_rss_mib,
        "min_system_available_mib": limits.min_system_available_mib,
        "sample_interval_s": limits.sample_interval_s,
        "violation_grace_samples": limits.violation_grace_samples,
        "sample_count": 0,
        "max_observed_process_tree_rss_mib": 0,
        "min_observed_system_available_mib": None,
        "violation_reason": None,
        "killed_process_ids": [],
    }


def _record_snapshot(state: dict, snapshot: dict) -> None:
    state["sample_count"] = int(state["sample_count"]) + 1
    rss = snapshot.get("process_tree_rss_mib")
    if isinstance(rss, (int, float)):
        state["max_observed_process_tree_rss_mib"] = max(
            int(state["max_observed_process_tree_rss_mib"]),
            int(rss),
        )
    available = snapshot.get("system_available_mib")
    if isinstance(available, (int, float)):
        current = state.get("min_observed_system_available_mib")
        state["min_observed_system_available_mib"] = int(available) if current is None else min(int(current), int(available))


def _resource_violation_reason(limits: ResourceLimits, snapshot: dict) -> str | None:
    rss = snapshot.get("process_tree_rss_mib")
    available = snapshot.get("system_available_mib")
    if isinstance(rss, (int, float)) and limits.max_process_tree_rss_mib > 0 and rss > limits.max_process_tree_rss_mib:
        return f"process_tree_rss_mib={int(rss)} > max_process_tree_rss_mib={limits.max_process_tree_rss_mib}"
    if (
        isinstance(available, (int, float))
        and limits.min_system_available_mib > 0
        and available < limits.min_system_available_mib
    ):
        return f"system_available_mib={int(available)} < min_system_available_mib={limits.min_system_available_mib}"
    return None


def _sample_resources(pid: int) -> dict:
    if os.name == "nt":
        return _sample_resources_windows(pid)
    return _sample_resources_procfs(pid)


def _sample_resources_windows(pid: int) -> dict:
    import ctypes
    from ctypes import wintypes

    class PROCESSENTRY32W(ctypes.Structure):
        _fields_ = [
            ("dwSize", wintypes.DWORD),
            ("cntUsage", wintypes.DWORD),
            ("th32ProcessID", wintypes.DWORD),
            ("th32DefaultHeapID", ctypes.c_size_t),
            ("th32ModuleID", wintypes.DWORD),
            ("cntThreads", wintypes.DWORD),
            ("th32ParentProcessID", wintypes.DWORD),
            ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", wintypes.DWORD),
            ("szExeFile", wintypes.WCHAR * 260),
        ]

    class PROCESS_MEMORY_COUNTERS(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
        ]

    class MEMORYSTATUSEX(ctypes.Structure):
        _fields_ = [
            ("dwLength", wintypes.DWORD),
            ("dwMemoryLoad", wintypes.DWORD),
            ("ullTotalPhys", ctypes.c_ulonglong),
            ("ullAvailPhys", ctypes.c_ulonglong),
            ("ullTotalPageFile", ctypes.c_ulonglong),
            ("ullAvailPageFile", ctypes.c_ulonglong),
            ("ullTotalVirtual", ctypes.c_ulonglong),
            ("ullAvailVirtual", ctypes.c_ulonglong),
            ("sullAvailExtendedVirtual", ctypes.c_ulonglong),
        ]

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    snapshot = kernel32.CreateToolhelp32Snapshot(0x00000002, 0)
    parent_by_pid: dict[int, int] = {}
    if snapshot != ctypes.c_void_p(-1).value:
        entry = PROCESSENTRY32W()
        entry.dwSize = ctypes.sizeof(entry)
        ok = kernel32.Process32FirstW(snapshot, ctypes.byref(entry))
        while ok:
            parent_by_pid[int(entry.th32ProcessID)] = int(entry.th32ParentProcessID)
            ok = kernel32.Process32NextW(snapshot, ctypes.byref(entry))
        kernel32.CloseHandle(snapshot)

    children_by_parent: dict[int, list[int]] = {}
    for child, parent in parent_by_pid.items():
        children_by_parent.setdefault(parent, []).append(child)
    tree = _expand_tree(pid, children_by_parent)
    PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    PROCESS_VM_READ = 0x0010
    total = 0
    alive_pids: list[int] = []
    for child_pid in tree:
        handle = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, False, child_pid)
        if not handle:
            continue
        counters = PROCESS_MEMORY_COUNTERS()
        counters.cb = ctypes.sizeof(counters)
        if psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
            total += int(counters.WorkingSetSize)
            alive_pids.append(child_pid)
        kernel32.CloseHandle(handle)

    memory = MEMORYSTATUSEX()
    memory.dwLength = ctypes.sizeof(memory)
    available = None
    if kernel32.GlobalMemoryStatusEx(ctypes.byref(memory)):
        available = int(memory.ullAvailPhys / (1024 * 1024))
    return {
        "process_tree_rss_mib": int(total / (1024 * 1024)),
        "system_available_mib": available,
        "process_ids": alive_pids,
    }


def _sample_resources_procfs(pid: int) -> dict:
    proc_root = Path("/proc")
    if not proc_root.exists():
        return {"process_tree_rss_mib": None, "system_available_mib": None, "process_ids": [pid]}
    children_by_parent: dict[int, list[int]] = {}
    for stat_path in proc_root.glob("[0-9]*/stat"):
        try:
            text = stat_path.read_text(encoding="utf-8", errors="ignore")
            parts = text.rsplit(") ", 1)[1].split()
            parent = int(parts[1])
            child = int(stat_path.parent.name)
        except Exception:
            continue
        children_by_parent.setdefault(parent, []).append(child)
    tree = _expand_tree(pid, children_by_parent)
    total_kib = 0
    alive = []
    for child_pid in tree:
        status = proc_root / str(child_pid) / "status"
        try:
            for line in status.read_text(encoding="utf-8", errors="ignore").splitlines():
                if line.startswith("VmRSS:"):
                    total_kib += int(line.split()[1])
                    alive.append(child_pid)
                    break
        except Exception:
            continue
    available = None
    try:
        for line in (proc_root / "meminfo").read_text(encoding="utf-8").splitlines():
            if line.startswith("MemAvailable:"):
                available = int(int(line.split()[1]) / 1024)
                break
    except Exception:
        pass
    return {
        "process_tree_rss_mib": int(total_kib / 1024),
        "system_available_mib": available,
        "process_ids": alive,
    }


def _expand_tree(pid: int, children_by_parent: Mapping[int, Sequence[int]]) -> list[int]:
    seen = set()
    stack = [int(pid)]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        stack.extend(int(child) for child in children_by_parent.get(current, []))
    return sorted(seen)


def _kill_process_tree(proc: subprocess.Popen) -> list[int]:
    pid = int(proc.pid)
    try:
        sample = _sample_resources(pid)
        killed = [int(item) for item in sample.get("process_ids", [])] or [pid]
    except Exception:
        killed = [pid]
    if os.name == "nt":
        try:
            subprocess.run(
                ["taskkill", "/PID", str(pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
                timeout=10,
            )
        except Exception:
            try:
                proc.kill()
            except Exception:
                pass
        return killed
    try:
        os.killpg(pid, signal.SIGKILL)
    except Exception:
        try:
            proc.kill()
        except Exception:
            pass
    return killed


def _env_bool(name: str, *, default: bool) -> bool:
    value = os.environ.get(name)
    if value is None:
        return default
    return value.strip().lower() not in {"0", "false", "no", "off", "never"}


def _env_int(name: str, default: int) -> int:
    try:
        return int(os.environ.get(name, str(default)))
    except ValueError:
        return default


def _env_float(name: str, default: float) -> float:
    try:
        return float(os.environ.get(name, str(default)))
    except ValueError:
        return default
