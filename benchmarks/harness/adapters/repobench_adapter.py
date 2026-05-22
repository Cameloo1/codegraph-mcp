from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Any

from benchmarks.harness.adapters.base import AdapterSetupStatus, read_jsonl
from benchmarks.harness.paths import fixture_path, resolve_benchmark_path, workspace_path


class RepoBenchAdapter:
    benchmark = "repobench"
    dataset_version = "tianyang/repobench_python_v1.1"

    def __init__(self, dataset_path: str | Path, language: str = "python"):
        self.dataset_path = resolve_benchmark_path(dataset_path)
        self.language = language

    def setup_status(self) -> AdapterSetupStatus:
        fixture = fixture_path("repobench", "repobench_tiny.jsonl")
        data_file = self._find_data_file()
        blockers: list[str] = []
        if data_file is None:
            blockers.append(
                "RepoBench v1.1 data is not present locally. Install the Hugging Face "
                "datasets package and export tianyang/repobench_python_v1.1 or "
                "tianyang/repobench_java_v1.1 into an ignored benchmark path."
            )
            return AdapterSetupStatus(
                benchmark=self.benchmark,
                setup_state="fixture_only" if fixture.exists() else "skipped_with_precise_blocker",
                status="blocked_manual_download",
                dataset_version=self.dataset_version,
                local_path=str(self.dataset_path),
                ready=False,
                smoke_ready=False,
                fixture_only=fixture.exists(),
                blockers=blockers,
                setup_commands=[
                    "python -m pip install datasets",
                    (
                        "python -c \"from datasets import load_dataset; "
                        "load_dataset('tianyang/repobench_python_v1.1')\""
                    ),
                    "export or cache the dataset under benchmarks/tracks/repobench/workspaces/repobench_data",
                ],
                notes=[
                    "The pinned RepoBench repository is source code plus metadata; the official v1.1 data is on Hugging Face."
                ],
            )
        return AdapterSetupStatus(
            benchmark=self.benchmark,
            setup_state="ready_for_full_run",
            status="ready_for_full_run",
            dataset_version=self.dataset_version,
            local_path=str(data_file),
            ready=True,
            smoke_ready=True,
            blockers=[],
            setup_commands=[],
            notes=[
                "RepoBench data file found; retrieval-context smoke and configured small runs can load real task metadata."
            ],
            details={"row_count": _count_jsonl_rows(data_file)},
        )

    def load_tasks(self, limit: int | None = None) -> list[dict]:
        data_file = self._find_data_file()
        if data_file is None:
            status = self.setup_status()
            raise RuntimeError("; ".join(status.blockers))
        rows = read_jsonl(data_file, limit=limit)
        return [self._map_row(row, index) for index, row in enumerate(rows)]

    def load_fixture_tasks(self, limit: int | None = None) -> list[dict]:
        fixture = fixture_path("repobench", "repobench_tiny.jsonl")
        rows = read_jsonl(fixture, limit=limit)
        return [self._map_row(row, index) for index, row in enumerate(rows)]

    def _find_data_file(self) -> Path | None:
        if self.dataset_path.is_file() and self.dataset_path.suffix == ".jsonl":
            return self.dataset_path
        if self.dataset_path.is_dir():
            candidates = sorted(self.dataset_path.rglob("*.jsonl"))
            for candidate in candidates:
                if self.language.lower() in str(candidate).lower() or len(candidates) == 1:
                    return candidate
        return None

    def _map_row(self, row: dict[str, Any], index: int) -> dict[str, Any]:
        metadata = row.get("metadata") if isinstance(row.get("metadata"), dict) else {}
        context_items = _context_items(row)
        gold_files = []
        gold_symbols = []
        for item in context_items:
            if not isinstance(item, dict):
                continue
            path = item.get("filename") or item.get("path") or item.get("file_path")
            if path:
                gold_files.append(str(path).replace("\\", "/"))
            identifier = item.get("identifier") or item.get("symbol") or item.get("name")
            if identifier:
                gold_symbols.append(str(identifier))
        task_id = str(row.get("task_id") or metadata.get("task_id") or f"repobench_{index}")
        prompt = row.get("prompt") or row.get("task")
        if not prompt:
            prompt = "Retrieve cross-file context for "
            prompt += str(row.get("file_path") or row.get("repo_name") or "RepoBench task")
        repo_path = _materialize_repobench_repo(task_id, row, context_items)
        query_terms = gold_symbols[:5]
        if row.get("file_path"):
            query_terms.append(Path(str(row["file_path"])).name)
        return {
            "task_id": task_id,
            "repo_kind": "repobench",
            "task": str(prompt),
            "repo_path": str(repo_path),
            "gold_files": gold_files,
            "gold_symbols": gold_symbols,
            "gold_spans": [],
            "forbidden_files": [],
            "forbidden_symbols": [],
            "expected_claimability": {"graph_proof_allowed": False, "text_evidence_allowed": True},
            "task_type": "repo_context_retrieval",
            "source": "repobench_real_or_fixture",
            "query_terms": query_terms,
            "metadata": {
                "repo_name": row.get("repo_name"),
                "target_file": row.get("file_path"),
                "dataset_level": row.get("level"),
                "materialized_repo": str(repo_path),
                "materialized_from_snippets": True,
            },
        }


def _context_items(row: dict[str, Any]) -> list[Any]:
    crossfile = row.get("crossfile_context")
    if isinstance(crossfile, dict) and isinstance(crossfile.get("list"), list):
        return list(crossfile["list"])
    context = row.get("context")
    if isinstance(context, list):
        return context
    if isinstance(context, dict) and isinstance(context.get("list"), list):
        return list(context["list"])
    return []


def _materialize_repobench_repo(task_id: str, row: dict[str, Any], context_items: list[Any]) -> Path:
    root = workspace_path("repobench", "materialized") / _safe_id(task_id)
    root.mkdir(parents=True, exist_ok=True)
    target = str(row.get("file_path") or "target.py").replace("\\", "/")
    target_text = "\n".join(
        str(part)
        for part in (row.get("import_statement"), row.get("cropped_code"), row.get("all_code"), row.get("next_line"))
        if part
    )
    _write_repo_file(root, target, target_text or "# RepoBench target file placeholder from official row metadata\n")
    for item in context_items:
        if not isinstance(item, dict):
            continue
        path = item.get("path") or item.get("filename") or item.get("file_path")
        snippet = item.get("snippet") or item.get("retrieved_chunk") or item.get("text")
        if path and snippet:
            _write_repo_file(root, str(path).replace("\\", "/"), str(snippet))
    return root


def _write_repo_file(root: Path, relative: str, text: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists() or path.read_text(encoding="utf-8", errors="ignore") != text:
        path.write_text(text, encoding="utf-8")


def _safe_id(value: str) -> str:
    return hashlib.sha1(value.encode("utf-8", errors="ignore")).hexdigest()[:16]


def _count_jsonl_rows(path: Path) -> int:
    with path.open("r", encoding="utf-8", errors="ignore") as handle:
        return sum(1 for line in handle if line.strip())
