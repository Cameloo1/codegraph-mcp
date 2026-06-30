#!/usr/bin/env python3
"""Generate a local linter-experience packet gallery from real agent-use runs.

The lab creates disposable fixture repos, uses the release `codegraph-mcp`
binary with an external `CODEGRAPH_AGENT_USE_DATA_ROOT`, runs
`status -> index -> validate-edit`, and records the actual packets, DB sizes,
timings, command logs, and lightweight HTML/Markdown summaries.

This is diagnostic tooling only. It does not make public benchmark claims and it
does not use repo-local `.codegraph` paths.
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "reports" / "audit" / "artifacts" / "linter_experience_lab" / "latest"
DEFAULT_SCRATCH_ROOT = Path(
    os.environ.get("CODEGRAPH_LINTER_LAB_SCRATCH", str(ROOT / "target" / "linter-experience-lab"))
)
DEFAULT_REPORT_MD = ROOT / "reports" / "audit" / "linter_experience_lab.md"
DEFAULT_REPORT_JSON = ROOT / "reports" / "audit" / "linter_experience_lab.json"
AGENT_USE_DATA_ROOT_ENV = "CODEGRAPH_AGENT_USE_DATA_ROOT"


@dataclass(frozen=True)
class ScenarioStep:
    name: str
    description: str
    updates: dict[str, str | None]
    changed_files: list[str]


@dataclass(frozen=True)
class Scenario:
    scenario_id: str
    title: str
    intent: str
    baseline_files: dict[str, str]
    steps: list[ScenarioStep]


@dataclass
class CommandRecord:
    name: str
    argv: list[str]
    cwd: str
    env: dict[str, str]
    started_at: str
    ended_at: str
    duration_ms: int
    exit_code: int | None
    timed_out: bool
    stdout_path: str
    stderr_path: str
    parsed_json_path: str | None
    json_parse_error: str | None

    @property
    def status(self) -> str:
        if self.timed_out:
            return "timeout"
        if self.exit_code == 0:
            return "success"
        return "failed"


SCENARIOS: list[Scenario] = [
    Scenario(
        scenario_id="python_noop_ok",
        title="Python no-op validation",
        intent="Baseline packet shape when the changed file has no semantic break.",
        baseline_files={
            "app.py": (
                "def known_transform(value):\n"
                "    return value + 1\n\n"
                "def run(value):\n"
                "    return known_transform(value)\n"
            ),
        },
        steps=[
            ScenarioStep(
                name="noop_validate",
                description="Run validate-edit against an unchanged, indexed file.",
                updates={},
                changed_files=["app.py"],
            ),
        ],
    ),
    Scenario(
        scenario_id="python_hallucinated_call_then_fix",
        title="Python hallucinated call then fix",
        intent="Show whether a newly invented call is surfaced, then whether the packet clears after repair.",
        baseline_files={
            "app.py": (
                "def known_transform(value):\n"
                "    return value + 1\n\n"
                "def run(value):\n"
                "    return known_transform(value)\n"
            ),
        },
        steps=[
            ScenarioStep(
                name="introduce_missing_call",
                description="Replace a known call with a newly invented function name.",
                updates={
                    "app.py": (
                        "def known_transform(value):\n"
                        "    return value + 1\n\n"
                        "def run(value):\n"
                        "    return invented_transform(value)\n"
                    ),
                },
                changed_files=["app.py"],
            ),
            ScenarioStep(
                name="fix_missing_call",
                description="Add the missing helper and rerun validation.",
                updates={
                    "app.py": (
                        "def known_transform(value):\n"
                        "    return value + 1\n\n"
                        "def invented_transform(value):\n"
                        "    return known_transform(value)\n\n"
                        "def run(value):\n"
                        "    return invented_transform(value)\n"
                    ),
                },
                changed_files=["app.py"],
            ),
        ],
    ),
    Scenario(
        scenario_id="rust_same_file_deleted_callee",
        title="Rust same-file deleted callee",
        intent="Show the packet for a changed file that removes a callee while leaving the call site behind.",
        baseline_files={
            "Cargo.toml": (
                "[package]\n"
                "name = \"cg_linter_fixture\"\n"
                "version = \"0.1.0\"\n"
                "edition = \"2021\"\n"
            ),
            "src/lib.rs": (
                "pub fn helper() -> i32 {\n"
                "    1\n"
                "}\n\n"
                "pub fn run() -> i32 {\n"
                "    helper()\n"
                "}\n"
            ),
        },
        steps=[
            ScenarioStep(
                name="delete_callee",
                description="Delete the helper definition but keep the call.",
                updates={
                    "src/lib.rs": (
                        "pub fn run() -> i32 {\n"
                        "    helper()\n"
                        "}\n"
                    ),
                },
                changed_files=["src/lib.rs"],
            ),
            ScenarioStep(
                name="restore_callee",
                description="Restore the helper definition and rerun validation.",
                updates={
                    "src/lib.rs": (
                        "pub fn helper() -> i32 {\n"
                        "    1\n"
                        "}\n\n"
                        "pub fn run() -> i32 {\n"
                        "    helper()\n"
                        "}\n"
                    ),
                },
                changed_files=["src/lib.rs"],
            ),
        ],
    ),
]


def utc_now() -> str:
    return datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")


def safe_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "_", value).strip("_") or "item"


def rel(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT)).replace("\\", "/")
    except ValueError:
        return str(path.resolve())


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def write_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")


def read_json_from_stdout(stdout: str) -> tuple[Any | None, str | None]:
    stripped = stdout.strip()
    if not stripped:
        return None, "stdout was empty"
    first = stripped.find("{")
    if first < 0:
        return None, "stdout did not contain a JSON object"
    try:
        return json.loads(stripped[first:]), None
    except json.JSONDecodeError as exc:
        return None, f"{exc.msg} at line {exc.lineno} column {exc.colno}"


def run_command(
    name: str,
    argv: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    log_dir: Path,
    timeout_s: int,
) -> tuple[CommandRecord, Any | None]:
    log_dir.mkdir(parents=True, exist_ok=True)
    stem = safe_name(name)
    stdout_path = log_dir / f"{stem}.stdout.txt"
    stderr_path = log_dir / f"{stem}.stderr.txt"
    parsed_json_path = log_dir / f"{stem}.json"
    started_at = utc_now()
    start = time.perf_counter()
    timed_out = False
    exit_code: int | None
    stdout = ""
    stderr = ""
    merged_env = os.environ.copy()
    merged_env.update(env)
    try:
        completed = subprocess.run(
            argv,
            cwd=str(cwd),
            env=merged_env,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout_s,
        )
        exit_code = completed.returncode
        stdout = completed.stdout
        stderr = completed.stderr
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        exit_code = None
        stdout = exc.stdout if isinstance(exc.stdout, str) else (exc.stdout or b"").decode("utf-8", "replace")
        stderr = exc.stderr if isinstance(exc.stderr, str) else (exc.stderr or b"").decode("utf-8", "replace")
        stderr += f"\n[TIMEOUT after {timeout_s}s]\n"
    duration_ms = int((time.perf_counter() - start) * 1000)
    ended_at = utc_now()
    write_text(stdout_path, stdout)
    write_text(stderr_path, stderr)
    parsed, parse_error = read_json_from_stdout(stdout)
    parsed_path_text: str | None = None
    if parsed is not None:
        write_json(parsed_json_path, parsed)
        parsed_path_text = rel(parsed_json_path)
    record = CommandRecord(
        name=name,
        argv=argv,
        cwd=str(cwd),
        env={key: env[key] for key in sorted(env)},
        started_at=started_at,
        ended_at=ended_at,
        duration_ms=duration_ms,
        exit_code=exit_code,
        timed_out=timed_out,
        stdout_path=rel(stdout_path),
        stderr_path=rel(stderr_path),
        parsed_json_path=parsed_path_text,
        json_parse_error=parse_error,
    )
    return record, parsed


def apply_files(repo: Path, files: dict[str, str | None]) -> None:
    for relative, content in files.items():
        path = repo / relative
        if content is None:
            if path.exists():
                path.unlink()
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")


def find_profile_db(profile_root: Path) -> Path | None:
    matches = sorted(profile_root.rglob("production-agent-use.sqlite"))
    if matches:
        return matches[0]
    sqlite_matches = sorted(profile_root.rglob("*.sqlite"))
    return sqlite_matches[0] if sqlite_matches else None


def file_tree_sizes(root: Path) -> dict[str, Any]:
    files = []
    total = 0
    if root.exists():
        for path in sorted(p for p in root.rglob("*") if p.is_file()):
            size = path.stat().st_size
            total += size
            files.append({"path": rel(path), "bytes": size})
    return {
        "root": rel(root),
        "total_bytes": total,
        "total_mib": round(total / 1024 / 1024, 3),
        "files": files[:80],
        "file_count": len(files),
    }


def sqlite_anatomy(db_path: Path | None) -> dict[str, Any]:
    if db_path is None or not db_path.exists():
        return {"available": False, "reason": "db_not_found"}
    result: dict[str, Any] = {
        "available": True,
        "db_path": rel(db_path),
        "db_bytes": db_path.stat().st_size,
        "db_mib": round(db_path.stat().st_size / 1024 / 1024, 3),
        "page_size": None,
        "page_count": None,
        "freelist_count": None,
        "tables": [],
        "interesting_counts": {},
    }
    try:
        conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
        try:
            page_size = conn.execute("PRAGMA page_size").fetchone()
            page_count = conn.execute("PRAGMA page_count").fetchone()
            freelist_count = conn.execute("PRAGMA freelist_count").fetchone()
            result["page_size"] = page_size[0] if page_size else None
            result["page_count"] = page_count[0] if page_count else None
            result["freelist_count"] = freelist_count[0] if freelist_count else None
            tables = [
                row[0]
                for row in conn.execute(
                    "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
                ).fetchall()
                if not row[0].startswith("sqlite_")
            ]
            for table in tables:
                count: int | str
                try:
                    count = int(conn.execute(f'SELECT COUNT(*) FROM "{table}"').fetchone()[0])
                except Exception as exc:
                    count = f"error: {exc}"
                result["tables"].append({"name": table, "rows": count})
                lowered = table.lower()
                if any(token in lowered for token in ["entit", "edge", "span", "unresolved", "candidate", "file", "fts"]):
                    result["interesting_counts"][table] = count
        finally:
            conn.close()
    except Exception as exc:
        result["available"] = False
        result["reason"] = str(exc)
    return result


def packet_summary(packet: Any) -> dict[str, Any]:
    if not isinstance(packet, dict):
        return {"available": False, "reason": "packet_not_json_object"}
    validation = packet.get("validation_packet")
    if not isinstance(validation, dict):
        validation = packet
    summary: dict[str, Any] = {
        "available": True,
        "root_status": packet.get("status"),
        "validation_status": validation.get("status") or validation.get("final_status"),
        "must_fix_before_continuing": packet.get("must_fix_before_continuing", validation.get("must_fix_before_continuing")),
        "hard_interrupt_available": packet.get("hard_interrupt_available", validation.get("hard_interrupt_available")),
        "changed_files": validation.get("changed_files") or packet.get("changed_files") or [],
        "severity_summary": validation.get("severity_summary", {}),
        "summary_counts_by_classification": validation.get("summary_counts_by_classification", {}),
        "summary_counts_by_rule_id": validation.get("summary_counts_by_rule_id", {}),
        "omitted_count": validation.get("omitted_count", packet.get("omitted_count")),
        "expansion_handle_count": len(validation.get("expansion_handles", []) or []),
        "packet_bytes": len(json.dumps(packet, ensure_ascii=True).encode("utf-8")),
        "top_findings": [],
    }
    for bucket in ["blocking_errors", "warnings", "unknowns", "diagnostics"]:
        entries = validation.get(bucket, [])
        if not isinstance(entries, list):
            continue
        for entry in entries[:3]:
            if not isinstance(entry, dict):
                continue
            spans = entry.get("source_spans") or entry.get("spans") or []
            summary["top_findings"].append(
                {
                    "bucket": bucket,
                    "rule_id": entry.get("validation_rule_id") or entry.get("rule_id"),
                    "classification": entry.get("classification"),
                    "message": entry.get("message") or entry.get("summary"),
                    "recommended_fix": entry.get("recommended_fix"),
                    "source_spans": spans[:2] if isinstance(spans, list) else spans,
                }
            )
    return summary


def render_html(report: dict[str, Any]) -> str:
    css = """
    body { margin: 0; font-family: Segoe UI, Arial, sans-serif; background: #070b12; color: #edf4ff; }
    main { max-width: 1180px; margin: 0 auto; padding: 42px 28px 70px; }
    h1 { font-size: 36px; margin: 0 0 8px; }
    h2 { margin-top: 34px; border-top: 1px solid #263244; padding-top: 24px; }
    .muted { color: #b7c4d6; }
    .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 14px; }
    .card { background: #101827; border: 1px solid #263244; border-radius: 8px; padding: 14px; }
    .metric { font-size: 24px; font-weight: 700; }
    table { width: 100%; border-collapse: collapse; margin: 12px 0; font-size: 14px; }
    th, td { border-bottom: 1px solid #263244; text-align: left; padding: 9px; vertical-align: top; }
    th { color: #dce8fa; }
    code, pre { background: #0c1320; color: #e7eefb; }
    pre { padding: 12px; overflow: auto; border: 1px solid #263244; border-radius: 8px; max-height: 520px; }
    details { margin: 10px 0; }
    summary { cursor: pointer; color: #7dd3fc; font-weight: 650; }
    .ok { color: #66d68f; }
    .warn { color: #ffd95a; }
    .bad { color: #ff8383; }
    .unknown { color: #c084fc; }
    """
    rows = []
    for scenario in report["scenarios"]:
        for step in scenario["steps"]:
            ps = step.get("packet_summary", {})
            status = ps.get("validation_status") or step.get("validate", {}).get("status")
            klass = "ok" if status == "ok" else "bad" if ps.get("hard_interrupt_available") else "warn" if status == "warning" else "unknown"
            rows.append(
                "<tr>"
                f"<td>{html.escape(scenario['scenario_id'])}</td>"
                f"<td>{html.escape(step['name'])}</td>"
                f"<td class='{klass}'>{html.escape(str(status))}</td>"
                f"<td>{html.escape(str(ps.get('hard_interrupt_available')))}</td>"
                f"<td>{html.escape(str(ps.get('packet_bytes')))}</td>"
                f"<td>{html.escape(str(step.get('validate', {}).get('duration_ms')))}</td>"
                "</tr>"
            )
    db_rows = []
    for scenario in report["scenarios"]:
        db = scenario.get("db_anatomy", {})
        db_rows.append(
            "<tr>"
            f"<td>{html.escape(scenario['scenario_id'])}</td>"
            f"<td>{html.escape(str(db.get('db_mib')))}</td>"
            f"<td>{html.escape(str(db.get('page_count')))}</td>"
            f"<td><pre>{html.escape(json.dumps(db.get('interesting_counts', {}), indent=2))}</pre></td>"
            "</tr>"
        )
    packet_blocks = []
    for scenario in report["scenarios"]:
        for step in scenario["steps"]:
            packet = step.get("packet")
            packet_text = json.dumps(packet, indent=2, ensure_ascii=True) if packet is not None else "null"
            packet_blocks.append(
                f"<details><summary>{html.escape(scenario['scenario_id'])} / {html.escape(step['name'])}</summary>"
                f"<pre>{html.escape(packet_text)}</pre></details>"
            )
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>CodeGraph Linter Experience Lab</title>
  <style>{css}</style>
</head>
<body>
<main>
  <h1>CodeGraph Linter Experience Lab</h1>
  <p class="muted">Local diagnostic packet gallery. No public benchmark claim.</p>
  <div class="grid">
    <div class="card"><div class="metric">{len(report['scenarios'])}</div><div class="muted">fixture repos</div></div>
    <div class="card"><div class="metric">{sum(len(s['steps']) for s in report['scenarios'])}</div><div class="muted">validate-edit packets</div></div>
    <div class="card"><div class="metric">{html.escape(str(report['totals'].get('db_mib_total')))}</div><div class="muted">MiB of profile artifacts</div></div>
    <div class="card"><div class="metric">{html.escape(str(report['totals'].get('hard_interrupt_packets')))}</div><div class="muted">hard-interrupt packets</div></div>
  </div>
  <h2>Run Summary</h2>
  <table><thead><tr><th>Scenario</th><th>Step</th><th>Status</th><th>Hard interrupt</th><th>Packet bytes</th><th>Validate ms</th></tr></thead><tbody>
  {''.join(rows)}
  </tbody></table>
  <h2>DB Anatomy</h2>
  <table><thead><tr><th>Scenario</th><th>DB MiB</th><th>Pages</th><th>Interesting row counts</th></tr></thead><tbody>
  {''.join(db_rows)}
  </tbody></table>
  <h2>Packet Gallery</h2>
  {''.join(packet_blocks)}
</main>
</body>
</html>
"""


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# CodeGraph Linter Experience Lab",
        "",
        "Local diagnostic packet gallery for the `agent-use validate-edit` linter experience.",
        "This is not a public benchmark claim and not a CodeGraph-over-rg claim.",
        "",
        f"- Generated: `{report['generated_at']}`",
        f"- Release binary: `{report['binary']}`",
        f"- Output root: `{report['output_root']}`",
        f"- HTML gallery: `{report['html_gallery']}`",
        f"- Scenarios: `{len(report['scenarios'])}`",
        f"- Validate packets: `{sum(len(s['steps']) for s in report['scenarios'])}`",
        f"- Total profile artifacts: `{report['totals'].get('db_mib_total')} MiB`",
        f"- Hard-interrupt packets: `{report['totals'].get('hard_interrupt_packets')}`",
        "",
        "## Scenario Summary",
        "",
        "| Scenario | Step | Validation status | Hard interrupt | Packet bytes | Validate ms |",
        "| --- | --- | --- | --- | ---: | ---: |",
    ]
    for scenario in report["scenarios"]:
        for step in scenario["steps"]:
            summary = step.get("packet_summary", {})
            lines.append(
                "| "
                + " | ".join(
                    [
                        scenario["scenario_id"],
                        step["name"],
                        str(summary.get("validation_status")),
                        str(summary.get("hard_interrupt_available")),
                        str(summary.get("packet_bytes")),
                        str(step.get("validate", {}).get("duration_ms")),
                    ]
                )
                + " |"
            )
    lines.extend(["", "## DB Anatomy", ""])
    for scenario in report["scenarios"]:
        db = scenario.get("db_anatomy", {})
        lines.extend(
            [
                f"### {scenario['scenario_id']}",
                "",
                f"- DB: `{db.get('db_path')}`",
                f"- DB MiB: `{db.get('db_mib')}`",
                f"- Profile artifact MiB: `{scenario.get('profile_artifacts', {}).get('total_mib')}`",
                f"- Interesting counts: `{json.dumps(db.get('interesting_counts', {}), sort_keys=True)}`",
                "",
            ]
        )
    lines.extend(["## What To Inspect", ""])
    lines.extend(
        [
            "- `packet_summary.validation_status`: what the agent would route on.",
            "- `hard_interrupt_available`: whether the agent must stop by default.",
            "- `top_findings`: exact spans, rule ids, and recommended fixes when present.",
            "- `db_anatomy.interesting_counts`: rough DB shape behind the packet.",
            "- `commands.jsonl`: exact argv, stdout/stderr files, exit codes, and timings.",
            "",
            "Known boundary: this lab verifies packet shape and local linter behavior on disposable fixtures. "
            "It does not prove public benchmark quality, real-agent patch quality, or production semantic completeness.",
            "",
        ]
    )
    return "\n".join(lines)


def run_scenario(args: argparse.Namespace, scenario: Scenario, output: Path, scratch_root: Path) -> dict[str, Any]:
    scenario_root = scratch_root / "workspaces" / scenario.scenario_id
    profile_root = scratch_root / "profiles" / scenario.scenario_id
    logs = output / "logs" / scenario.scenario_id
    if scenario_root.exists():
        shutil.rmtree(scenario_root)
    if profile_root.exists():
        shutil.rmtree(profile_root)
    scenario_root.mkdir(parents=True)
    profile_root.mkdir(parents=True)
    apply_files(scenario_root, scenario.baseline_files)
    env = {AGENT_USE_DATA_ROOT_ENV: str(profile_root)}
    binary = str(Path(args.binary).resolve())
    commands: list[CommandRecord] = []
    status_before, _ = run_command(
        "status_before_index",
        [binary, "agent-use", "status", "--repo", str(scenario_root), "--json"],
        cwd=ROOT,
        env=env,
        log_dir=logs,
        timeout_s=args.command_timeout_s,
    )
    commands.append(status_before)
    index_record, index_json = run_command(
        "index",
        [binary, "agent-use", "index", "--repo", str(scenario_root), "--json"],
        cwd=ROOT,
        env=env,
        log_dir=logs,
        timeout_s=args.index_timeout_s,
    )
    commands.append(index_record)
    status_after, status_json = run_command(
        "status_after_index",
        [binary, "agent-use", "status", "--repo", str(scenario_root), "--json"],
        cwd=ROOT,
        env=env,
        log_dir=logs,
        timeout_s=args.command_timeout_s,
    )
    commands.append(status_after)
    db_path = find_profile_db(profile_root)
    steps = []
    for step in scenario.steps:
        apply_files(scenario_root, step.updates)
        argv = [binary, "agent-use", "validate-edit", "--repo", str(scenario_root)]
        for changed in step.changed_files:
            argv.extend(["--changed", changed])
        argv.append("--agent-json")
        validate_record, validate_json = run_command(
            f"validate_{step.name}",
            argv,
            cwd=ROOT,
            env=env,
            log_dir=logs,
            timeout_s=args.command_timeout_s,
        )
        commands.append(validate_record)
        step_dir = output / "packets" / scenario.scenario_id
        if validate_json is not None:
            write_json(step_dir / f"{step.name}.compact.json", validate_json)
        steps.append(
            {
                "name": step.name,
                "description": step.description,
                "changed_files": step.changed_files,
                "validate": validate_record.__dict__ | {"status": validate_record.status},
                "packet_path": rel(step_dir / f"{step.name}.compact.json") if validate_json is not None else None,
                "packet": validate_json,
                "packet_summary": packet_summary(validate_json),
            }
        )
    return {
        "scenario_id": scenario.scenario_id,
        "title": scenario.title,
        "intent": scenario.intent,
        "workspace": rel(scenario_root),
        "profile_root": rel(profile_root),
        "status_before_index": status_before.__dict__ | {"status": status_before.status},
        "index": index_record.__dict__ | {"status": index_record.status},
        "status_after_index": status_after.__dict__ | {"status": status_after.status},
        "index_packet": index_json,
        "status_packet": status_json,
        "db_anatomy": sqlite_anatomy(db_path),
        "profile_artifacts": file_tree_sizes(profile_root),
        "steps": steps,
        "commands": [record.__dict__ | {"status": record.status} for record in commands],
    }


def run_lab(args: argparse.Namespace) -> dict[str, Any]:
    output = Path(args.output)
    if not output.is_absolute():
        output = ROOT / output
    scratch_root = Path(args.scratch_root)
    if not scratch_root.is_absolute():
        scratch_root = ROOT / scratch_root
    if args.clean and output.exists():
        shutil.rmtree(output)
    if args.clean and scratch_root.exists():
        shutil.rmtree(scratch_root)
    output.mkdir(parents=True, exist_ok=True)
    scratch_root.mkdir(parents=True, exist_ok=True)
    scenario_reports = [run_scenario(args, scenario, output, scratch_root) for scenario in SCENARIOS]
    commands_jsonl = output / "commands.jsonl"
    with commands_jsonl.open("w", encoding="utf-8") as handle:
        for scenario in scenario_reports:
            for command in scenario["commands"]:
                handle.write(json.dumps(command, ensure_ascii=True) + "\n")
    total_artifact_bytes = sum(int(s["profile_artifacts"]["total_bytes"]) for s in scenario_reports)
    hard_interrupt_packets = 0
    for scenario in scenario_reports:
        for step in scenario["steps"]:
            if step.get("packet_summary", {}).get("hard_interrupt_available"):
                hard_interrupt_packets += 1
    report: dict[str, Any] = {
        "schema_version": 1,
        "generated_at": utc_now(),
        "status": "complete",
        "public_claim": False,
        "normal_dot_codegraph_mutated": (ROOT / ".codegraph").exists(),
        "binary": str(Path(args.binary).resolve()),
        "output_root": rel(output),
        "scratch_root": str(scratch_root.resolve()),
        "commands_jsonl": rel(commands_jsonl),
        "scenarios": scenario_reports,
        "totals": {
            "profile_artifact_bytes_total": total_artifact_bytes,
            "db_mib_total": round(total_artifact_bytes / 1024 / 1024, 3),
            "hard_interrupt_packets": hard_interrupt_packets,
        },
    }
    html_path = output / "linter_experience_lab.html"
    report["html_gallery"] = rel(html_path)
    write_text(html_path, render_html(report))
    write_json(output / "linter_experience_lab.json", report)
    write_text(output / "linter_experience_lab.md", render_markdown(report))
    write_json(Path(args.report_json), report)
    write_text(Path(args.report_md), render_markdown(report))
    return report


def parse_args(argv: list[str]) -> argparse.Namespace:
    default_binary = ROOT / "target" / "release" / ("codegraph-mcp.exe" if os.name == "nt" else "codegraph-mcp")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(default_binary), help="Release codegraph-mcp binary to test.")
    parser.add_argument("--output", default=str(DEFAULT_OUTPUT), help="Artifact output directory.")
    parser.add_argument("--report-md", default=str(DEFAULT_REPORT_MD), help="Stable markdown report path.")
    parser.add_argument("--report-json", default=str(DEFAULT_REPORT_JSON), help="Stable JSON report path.")
    parser.add_argument("--scratch-root", default=str(DEFAULT_SCRATCH_ROOT), help="Short disposable workspace/profile root.")
    parser.add_argument("--index-timeout-s", type=int, default=120)
    parser.add_argument("--command-timeout-s", type=int, default=45)
    parser.add_argument("--clean", action="store_true", help="Remove the output directory before running.")
    parser.add_argument("--list-scenarios", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.list_scenarios:
        for scenario in SCENARIOS:
            print(f"{scenario.scenario_id}: {scenario.title}")
        return 0
    binary = Path(args.binary)
    if not binary.exists():
        print(f"release binary not found: {binary}", file=sys.stderr)
        return 2
    report = run_lab(args)
    print(json.dumps({"status": report["status"], "report": rel(Path(args.report_json)), "html": report["html_gallery"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
