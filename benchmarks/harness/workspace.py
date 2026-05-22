from __future__ import annotations

import hashlib
from pathlib import Path


def ensure_dir(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    return path


def safe_slug(value: str) -> str:
    cleaned = "".join(ch if ch.isalnum() else "-" for ch in value.lower()).strip("-")
    return cleaned or "item"


def stable_id(value: str, length: int = 12) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()[:length]


def resolve_repo_path(repo_path: str | None, root: Path) -> Path:
    if not repo_path:
        return root
    path = Path(repo_path)
    if path.is_absolute():
        return path
    return (root / path).resolve()

