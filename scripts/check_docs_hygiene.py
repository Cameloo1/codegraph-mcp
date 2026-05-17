#!/usr/bin/env python3
"""Fail on public-doc references to local/generated development artifacts."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

PUBLIC_DOCS = [
    "README.md",
    "docs/architecture.md",
    "docs/benchmark-guide.md",
    "docs/cli-reference.md",
    "docs/codegraphcontext-comparison.md",
    "docs/guardrails.md",
    "docs/install.md",
    "docs/language-frontends.md",
    "docs/mcp-reference.md",
    "docs/operational-profiles.md",
    "docs/quality-gates.md",
    "docs/quickstart.md",
    "docs/troubleshooting.md",
]

STABLE_REPORT_PREFIXES = (
    "reports/final/comprehensive_benchmark_latest.",
    "reports/final/intended_tool_quality_gate.",
    "reports/final/lifecycle_quality_gate.",
    "reports/final/manual_relation_precision.",
    "reports/comparison/codegraph_vs_cgc_latest.",
    "reports/baselines/",
)

TRACKED_DENYLIST = (
    "reports/final/artifacts/",
    "reports/final/lifecycle_quality_gate_artifacts/",
    "reports/comparison/artifacts/",
    "reports/comparison/fixtures/",
    "reports/comparison/normalized_outputs/",
    "reports/comparison/raw_artifacts/",
    "reports/comparison/cgc_recovery/",
    "reports/comparison/cgc_fork_pr_readiness/",
    "reports/diagnostic_lab/",
)

TRACKED_EXACT_DENYLIST = {
    "reports/comparison/run.json",
    "reports/comparison/summary.md",
    "reports/comparison/per_task.jsonl",
}

PUBLIC_DOC_PATTERNS = [
    (re.compile(r"C:\\Users\\wamin", re.IGNORECASE), "machine-local user path"),
    (re.compile(r"raw_artifacts", re.IGNORECASE), "raw artifact directory"),
    (re.compile(r"normalized_outputs", re.IGNORECASE), "normalized run payload directory"),
    (re.compile(r"reports/final/artifacts", re.IGNORECASE), "final artifact payload path"),
    (re.compile(r"reports\\final\\artifacts", re.IGNORECASE), "final artifact payload path"),
    (re.compile(r"target[/\\]phase\d+", re.IGNORECASE), "phase target output"),
    (re.compile(r"\bPhase\s+\d{2}\b", re.IGNORECASE), "phase diary language"),
    (re.compile(r"\bNo subagents\b", re.IGNORECASE), "agent prompt policy in public docs"),
]


def git_ls_files() -> list[str]:
    result = subprocess.run(
        ["git", "ls-files"],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return [line.strip().replace("\\", "/") for line in result.stdout.splitlines() if line.strip()]


def is_allowed_report(path: str) -> bool:
    if path == "reports/audit/README.md":
        return True
    return path.startswith(STABLE_REPORT_PREFIXES)


def check_tracked_files(paths: list[str]) -> list[str]:
    failures: list[str] = []
    for path in paths:
        if path in TRACKED_EXACT_DENYLIST:
            failures.append(f"{path}: tracked generated comparison payload")
        if path.startswith(TRACKED_DENYLIST):
            failures.append(f"{path}: tracked generated report/artifact payload")
        if path.startswith("reports/audit/") and path != "reports/audit/README.md":
            failures.append(f"{path}: tracked audit report should be promoted explicitly or ignored")
        if (
            path.startswith("reports/")
            and path.endswith((".md", ".json", ".jsonl"))
            and not is_allowed_report(path)
            and not path.startswith("reports/smoke/")
        ):
            if path not in TRACKED_EXACT_DENYLIST and not path.startswith(TRACKED_DENYLIST):
                failures.append(f"{path}: non-stable report tracked outside the public summary set")
    return failures


def check_public_docs() -> list[str]:
    failures: list[str] = []
    for rel in PUBLIC_DOCS:
        path = ROOT / rel
        if not path.exists():
            failures.append(f"{rel}: missing public doc")
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for lineno, line in enumerate(text.splitlines(), start=1):
            for pattern, label in PUBLIC_DOC_PATTERNS:
                if pattern.search(line):
                    failures.append(f"{rel}:{lineno}: {label}: {line.strip()}")
    return failures


def main() -> int:
    failures = check_tracked_files(git_ls_files())
    failures.extend(check_public_docs())
    if failures:
        print("Documentation hygiene check failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print("Documentation hygiene check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
