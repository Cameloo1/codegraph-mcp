from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Any

from benchmarks.harness.adapters.base import AdapterSetupStatus, read_jsonl
from benchmarks.harness.paths import fixture_path, resolve_benchmark_path, upstream_path, workspace_path


class CrossCodeEvalAdapter:
    benchmark = "crosscodeeval"
    dataset_version = "amazon-science/cceval"

    def __init__(self, dataset_path: str | Path, languages: list[str] | None = None):
        self.dataset_path = resolve_benchmark_path(dataset_path)
        self.languages = languages or ["python", "java", "typescript", "csharp"]

    def setup_status(self) -> AdapterSetupStatus:
        data_root = self._data_root()
        data_files = self._find_data_files()
        tree_sitter_build = resolve_benchmark_path(upstream_path("crosscodeeval", "cceval", "build"))
        expected_parser_libs = [
            tree_sitter_build / "python-lang-parser.so",
            tree_sitter_build / "java-lang-parser.so",
            tree_sitter_build / "typescript-lang-parser.so",
            tree_sitter_build / "csharp-lang-parser.so",
        ]
        parser_libs = [path for path in expected_parser_libs if path.exists()]
        tree_sitter_ready = len(parser_libs) == len(expected_parser_libs)
        languages_present = sorted({path.parent.name for path in data_files})
        if not data_files:
            return AdapterSetupStatus(
                benchmark=self.benchmark,
                setup_state="fixture_only"
                if fixture_path("crosscodeeval", "crosscodeeval_tiny.jsonl").exists()
                else "skipped_with_precise_blocker",
                status="blocked_manual_download",
                dataset_version=self.dataset_version,
                local_path=str(self.dataset_path),
                ready=False,
                smoke_ready=False,
                fixture_only=fixture_path("crosscodeeval", "crosscodeeval_tiny.jsonl").exists(),
                blockers=[
                    "CrossCodeEval data is not extracted. Run tar -xJf data/crosscodeeval_data.tar.xz -C data/ inside the pinned cceval checkout."
                ],
                setup_commands=[
                    "tar -xJf benchmarks/tracks/crosscodeeval/upstream/cceval/data/crosscodeeval_data.tar.xz -C benchmarks/tracks/crosscodeeval/upstream/cceval/data",
                    "bash scripts/build_treesitter.sh",
                ],
            )
        blockers: list[str] = []
        if not tree_sitter_ready:
            blockers.append(
                "CrossCodeEval data is extracted, but official-style generation scoring is blocked until tree-sitter libraries are built."
            )
        status = "ready_for_official_smoke" if tree_sitter_ready else "ready_for_retrieval_blocked_official_generation"
        setup_state = "ready_for_official_smoke" if tree_sitter_ready else "ready_for_retrieval_full_run"
        return AdapterSetupStatus(
            benchmark=self.benchmark,
            setup_state=setup_state,
            status=status,
            dataset_version=self.dataset_version,
            local_path=str(data_root),
            ready=True,
            smoke_ready=True,
            fixture_only=False,
            blockers=blockers,
            setup_commands=[] if not blockers else [
                "bash scripts/build_treesitter.sh",
                (
                    "Windows fallback: call Visual Studio Build Tools vcvars64.bat, ensure rc.exe is available, "
                    "then run python scripts/build_ts_lib.py"
                ),
                "Docker fallback: benchmarks/tracks/crosscodeeval/scripts/setup_docker.ps1",
            ],
            notes=[
                "Extracted CrossCodeEval JSONL data is enough for adapter smoke and retrieval-context diagnostics.",
                (
                    "Tree-sitter parser libraries are built and parser-load smoked."
                    if tree_sitter_ready
                    else "Official generation scoring still requires upstream dependencies and tree-sitter build."
                ),
                "Full CrossCodeEval generation quality still requires a configured model/inference path; retrieval-context scoring remains diagnostic.",
            ],
            details={
                "sample_file": str(data_files[0]),
                "language_files": [str(path) for path in data_files],
                "languages_present": languages_present,
                "tree_sitter_build_present": tree_sitter_ready,
                "parser_libs": [str(path) for path in parser_libs],
                "missing_parser_libs": [str(path) for path in expected_parser_libs if not path.exists()],
            },
        )

    def load_tasks(self, limit: int | None = None) -> list[dict]:
        data_files = self._find_data_files()
        if not data_files:
            status = self.setup_status()
            raise RuntimeError("; ".join(status.blockers))
        tasks: list[dict] = []
        per_file_limit = limit if limit is not None else None
        for data_file in data_files:
            language = data_file.parent.name
            rows = read_jsonl(data_file, limit=per_file_limit)
            for row in rows:
                tasks.append(self._map_row(row, len(tasks), language=language))
                if limit is not None and len(tasks) >= limit:
                    return tasks
        return tasks

    def load_fixture_tasks(self, limit: int | None = None) -> list[dict]:
        fixture = fixture_path("crosscodeeval", "crosscodeeval_tiny.jsonl")
        rows = read_jsonl(fixture, limit=limit)
        return [self._map_row(row, index) for index, row in enumerate(rows)]

    def _data_root(self) -> Path:
        if (self.dataset_path / "python").exists():
            return self.dataset_path
        if (self.dataset_path / "data" / "python").exists():
            return self.dataset_path / "data"
        if (self.dataset_path / "crosscodeeval_data" / "python").exists():
            return self.dataset_path / "crosscodeeval_data"
        return self.dataset_path

    def _find_data_files(self) -> list[Path]:
        root = self._data_root()
        matches = []
        for language in self.languages:
            candidate = root / language / "line_completion_rg1_unixcoder_cosine_sim.jsonl"
            if candidate.exists():
                matches.append(candidate)
        if matches:
            return matches
        candidates = sorted(root.rglob("line_completion_*_unixcoder_cosine_sim.jsonl")) if root.exists() else []
        return candidates

    def _map_row(self, row: dict[str, Any], index: int, language: str | None = None) -> dict[str, Any]:
        metadata = row.get("metadata") if isinstance(row.get("metadata"), dict) else {}
        context = row.get("crossfile_context")
        context_items = context.get("list", []) if isinstance(context, dict) else []
        gold_files = []
        for item in context_items:
            if isinstance(item, dict) and item.get("filename"):
                gold_files.append(str(item["filename"]).replace("\\", "/"))
        task_id = str(metadata.get("task_id") or row.get("task_id") or f"crosscodeeval_{index}")
        repo_path = _materialize_crosscodeeval_repo(task_id, row, context_items)
        query_terms = [Path(file).name for file in gold_files[:5]]
        groundtruth = row.get("groundtruth")
        if groundtruth:
            query_terms.append(str(groundtruth).split("(")[0].strip()[:80])
        return {
            "task_id": task_id,
            "repo_kind": "crosscodeeval",
            "task": str(row.get("prompt") or "CrossCodeEval cross-file completion context retrieval task"),
            "repo_path": str(repo_path),
            "gold_files": gold_files,
            "gold_symbols": [],
            "gold_spans": [],
            "forbidden_files": [],
            "forbidden_symbols": [],
            "expected_claimability": {"graph_proof_allowed": False, "text_evidence_allowed": True},
            "task_type": "cross_file_context_retrieval",
            "source": "crosscodeeval_real_or_fixture",
            "query_terms": [term for term in query_terms if term],
            "metadata": {
                "language": language or row.get("language"),
                "repository": metadata.get("repository"),
                "target_file": metadata.get("file"),
                "materialized_repo": str(repo_path),
                "materialized_from_snippets": True,
            },
        }


def _materialize_crosscodeeval_repo(task_id: str, row: dict[str, Any], context_items: list[Any]) -> Path:
    metadata = row.get("metadata") if isinstance(row.get("metadata"), dict) else {}
    root = workspace_path("crosscodeeval", "materialized") / _safe_id(task_id)
    root.mkdir(parents=True, exist_ok=True)
    target = str(metadata.get("file") or "target.py").replace("\\", "/")
    target_text = "\n".join(str(part) for part in (row.get("prompt"), row.get("groundtruth"), row.get("right_context")) if part)
    _write_repo_file(root, target, target_text or "# CrossCodeEval target file placeholder from official row metadata\n")
    for item in context_items:
        if not isinstance(item, dict):
            continue
        path = item.get("filename") or item.get("path") or item.get("file_path")
        snippet = item.get("retrieved_chunk") or item.get("snippet") or item.get("text")
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
