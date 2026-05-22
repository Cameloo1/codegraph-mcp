from __future__ import annotations

from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class AdapterSetupStatus:
    benchmark: str
    setup_state: str
    status: str
    dataset_version: str
    local_path: str
    ready: bool = False
    smoke_ready: bool = False
    fixture_only: bool = False
    blockers: list[str] = field(default_factory=list)
    setup_commands: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)
    details: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def read_jsonl(path: Path, limit: int | None = None) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    import json

    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if limit is not None and len(rows) >= limit:
                break
            text = line.strip()
            if not text:
                continue
            rows.append(json.loads(text))
    return rows


def first_existing(paths: list[Path]) -> Path | None:
    for path in paths:
        if path.exists():
            return path
    return None
