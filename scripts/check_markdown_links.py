#!/usr/bin/env python3
"""Validate local Markdown links in README.md and docs/*.md.

External URLs are reported as skipped. Local file paths are checked with
case-sensitive path traversal so Windows runs still catch Linux casing issues.
Local Markdown anchors are checked where the target file can be parsed.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote


REPO_ROOT = Path(__file__).resolve().parents[1]
LINK_RE = re.compile(r"(!?)\[([^\]]+)\]\(([^)\n]+)\)")
REF_RE = re.compile(r"^\s*\[[^\]]+\]:\s*(\S+)")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
SCHEME_RE = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


def strip_code_fences(text: str) -> list[tuple[int, str]]:
    rows: list[tuple[int, str]] = []
    in_fence = False
    for line_no, line in enumerate(text.splitlines(), start=1):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if not in_fence:
            rows.append((line_no, line))
    return rows


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
    return target.strip()


def is_external(target: str) -> bool:
    return bool(SCHEME_RE.match(target)) or target.startswith("//")


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


def slugify_heading(text: str) -> str:
    text = re.sub(r"`([^`]*)`", r"\1", text)
    text = re.sub(r"<[^>]+>", "", text)
    text = text.strip().lower()
    text = re.sub(r"[^\w\s-]", "", text)
    text = re.sub(r"\s+", "-", text)
    return text.strip("-")


def anchors_for(path: Path) -> set[str]:
    anchors: set[str] = set()
    seen: dict[str, int] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except UnicodeDecodeError:
        return anchors
    for line in lines:
        match = HEADING_RE.match(line)
        if not match:
            continue
        base = slugify_heading(match.group(2))
        if not base:
            continue
        count = seen.get(base, 0)
        seen[base] = count + 1
        anchors.add(base if count == 0 else f"{base}-{count}")
    return anchors


def markdown_files() -> list[Path]:
    files = [REPO_ROOT / "README.md"]
    docs_dir = REPO_ROOT / "docs"
    if docs_dir.exists():
        files.extend(sorted(docs_dir.rglob("*.md")))
    return files


def validate() -> int:
    errors: list[str] = []
    skipped_external: list[str] = []
    checked_links = 0

    for md_path in markdown_files():
        rel_md = md_path.relative_to(REPO_ROOT).as_posix()
        text = md_path.read_text(encoding="utf-8")
        rows = strip_code_fences(text)
        candidates: list[tuple[int, str]] = []
        for line_no, line in rows:
            candidates.extend((line_no, split_target(match.group(3))) for match in LINK_RE.finditer(line))
            ref_match = REF_RE.match(line)
            if ref_match:
                candidates.append((line_no, split_target(ref_match.group(1))))

        for line_no, target in candidates:
            if not target or is_external(target):
                if target:
                    skipped_external.append(target)
                continue

            checked_links += 1
            path_part, _, anchor = target.partition("#")
            path_part = unquote(path_part)
            anchor = unquote(anchor).lower()

            if path_part:
                normalized = path_part.replace("\\", "/")
                target_path = (md_path.parent / normalized).resolve()
            else:
                target_path = md_path.resolve()

            if not case_sensitive_exists(target_path):
                errors.append(f"{rel_md}:{line_no}: missing or case-mismatched link target `{target}`")
                continue

            if anchor and target_path.suffix.lower() == ".md":
                if anchor not in anchors_for(target_path):
                    errors.append(f"{rel_md}:{line_no}: missing anchor `#{anchor}` in `{target}`")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        print(f"checked local markdown links: {checked_links}", file=sys.stderr)
        print(f"skipped external links: {len(skipped_external)}", file=sys.stderr)
        return 1

    print(f"markdown link check passed: {checked_links} local links checked, {len(skipped_external)} external links skipped")
    return 0


if __name__ == "__main__":
    raise SystemExit(validate())
