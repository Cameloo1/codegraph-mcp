#!/usr/bin/env python3
"""Validate README public assets and stable report references.

The README should explain product behavior with stable public assets. It should
not link generated DBs, raw run directories, or diagnostic-only artifacts.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
README = REPO_ROOT / "README.md"

REQUIRED_README_ASSETS = [
    "docs/assets/readme/title-pic.jpeg",
    "docs/assets/readme/agent_use_loop.svg",
    "docs/assets/readme/codegraph_terminal.svg",
]

DISALLOWED_README_TARGETS = [
    "docs/assets/readme/agent_readiness_manifest.json",
    "docs/assets/readme/evidence_safety.png",
    "docs/assets/readme/evidence_boundary_ladder.svg",
    "docs/assets/readme/production_profile_lifecycle.svg",
    "docs/assets/readme/verified_surface_summary.svg",
]

MARKDOWN_LINK_RE = re.compile(r"(?<!!)\[[^\]]+\]\(([^)\n]+)\)")
REPORT_PATH_RE = re.compile(
    r"reports[\\/](?:final|comparison)[\\/][A-Za-z0-9_.\\/\-]+(?:\.md|\.json|\.jsonl)"
)
PUBLIC_ASSET_TEXT_PATTERNS = [
    (re.compile(r"C:\\Users\\wamin", re.IGNORECASE), "machine-local user path"),
    (re.compile(r"/mnt/host/c/Users/wamin", re.IGNORECASE), "machine-local host path"),
    (re.compile(r"\bcodegraph-mcp\s+--query\b", re.IGNORECASE), "unsupported top-level --query command"),
    (re.compile(r"\bcargo\s+run\s+--release\s+--bin\s+codegraph-mcp\s+--\s+--query\b", re.IGNORECASE), "unsupported cargo -- --query command"),
]


def split_target(raw: str) -> str:
    target = raw.strip()
    if target.startswith("<"):
        end = target.find(">")
        if end != -1:
            return target[1:end]
    for quote in (' "', " '"):
        pos = target.find(quote)
        if pos != -1:
            target = target[:pos]
    return target.split("#", 1)[0].strip()


def case_sensitive_exists(path: Path) -> bool:
    if not path.exists():
        return False
    try:
        rel = path.resolve().relative_to(REPO_ROOT.resolve())
    except ValueError:
        return False
    current = REPO_ROOT.resolve()
    for part in rel.parts:
        names = {entry.name: entry for entry in current.iterdir()}
        if part not in names:
            return False
        current = names[part]
    return True


def normalize(path: str) -> str:
    return path.strip().replace("\\", "/").strip(".,;:)")


def readme_report_paths(text: str) -> set[str]:
    paths: set[str] = set()

    for match in MARKDOWN_LINK_RE.finditer(text):
        target = normalize(split_target(match.group(1)))
        if target.startswith(("reports/final/", "reports/comparison/")):
            paths.add(target)

    for match in REPORT_PATH_RE.finditer(text):
        paths.add(normalize(match.group(0)))

    return paths


def validate() -> int:
    text = README.read_text(encoding="utf-8")
    paths = readme_report_paths(text)
    errors: list[str] = []

    for required in REQUIRED_README_ASSETS:
        if required not in text.replace("\\", "/"):
            errors.append(f"README does not reference required public asset `{required}`")
        elif not case_sensitive_exists(REPO_ROOT / required):
            errors.append(f"README public asset is missing or case-mismatched: `{required}`")
        elif required.endswith(".svg"):
            asset_text = (REPO_ROOT / required).read_text(encoding="utf-8", errors="replace")
            for pattern, label in PUBLIC_ASSET_TEXT_PATTERNS:
                if pattern.search(asset_text):
                    errors.append(f"README public asset `{required}` contains {label}")

    for disallowed in DISALLOWED_README_TARGETS:
        if disallowed in text.replace("\\", "/"):
            errors.append(f"README references retired public asset `{disallowed}`")

    for path in sorted(paths):
        if "/artifacts/" in path:
            errors.append(
                f"README links generated raw artifact `{path}`; link a stable report or document regeneration instead"
            )
            continue
        target = REPO_ROOT / path
        if not case_sensitive_exists(target):
            errors.append(f"README artifact path is missing or case-mismatched: `{path}`")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        print(f"README report paths found: {len(paths)}", file=sys.stderr)
        return 1

    print(
        "README artifact check passed: "
        f"{len(REQUIRED_README_ASSETS)} public assets and {len(paths)} report paths checked"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(validate())
