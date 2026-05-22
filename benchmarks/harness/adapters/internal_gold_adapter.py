from __future__ import annotations

import json
from pathlib import Path
from typing import Iterable

from benchmarks.harness.paths import resolve_benchmark_path
from benchmarks.harness.schema import validate_task


class InternalGoldAdapter:
    benchmark = "internal"
    dataset_version = "internal_gold_v1"

    def __init__(self, path: str | Path):
        self.path = resolve_benchmark_path(path)

    def load_tasks(self, limit: int | None = None) -> list[dict]:
        if not self.path.exists():
            raise FileNotFoundError(f"internal gold dataset not found: {self.path}")
        tasks: list[dict] = []
        paths = sorted(self.path.glob("*.jsonl")) if self.path.is_dir() else [self.path]
        for path in paths:
            with path.open("r", encoding="utf-8") as handle:
                for line_number, line in enumerate(handle, 1):
                    if not line.strip():
                        continue
                    task = json.loads(line)
                    errors = validate_task(task)
                    if errors:
                        joined = "; ".join(errors)
                        raise ValueError(f"{path}:{line_number}: {joined}")
                    tasks.append(task)
                    if limit is not None and len(tasks) >= limit:
                        return tasks
        return tasks


def iter_jsonl(path: Path) -> Iterable[dict]:
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                yield json.loads(line)
