from __future__ import annotations

from typing import Any

from benchmarks.harness.scoring.context_recall import normalize_path
from benchmarks.harness.scoring.hallucination import nonexistent_symbol_refs
from benchmarks.harness.scoring.patch_outcome import (
    changed_files_from_patch,
    patch_applied,
    wrong_file_edits,
)


def packet_context_files(packet: dict[str, Any]) -> list[str]:
    files: list[str] = []
    for value in packet.get("files", []):
        file_path = _file_value(value)
        if file_path and file_path not in files:
            files.append(file_path)
    for snippet in packet.get("snippets", []):
        if isinstance(snippet, dict):
            file_path = _file_value(snippet)
            if file_path and file_path not in files:
                files.append(file_path)
    return files


def changed_files_missing_from_context(patch_text: str, packet: dict[str, Any]) -> list[str]:
    context = {normalize_path(path) for path in packet_context_files(packet)}
    return [path for path in changed_files_from_patch(patch_text) if normalize_path(path) not in context]


def rg_flood_count(packet: dict[str, Any]) -> int:
    risks = packet.get("risks", [])
    if not isinstance(risks, list):
        return 0
    return sum(1 for risk in risks if isinstance(risk, dict) and risk.get("kind") == "rg_flood")


def evidence_alignment_summary(
    *,
    patch_text: str,
    packet: dict[str, Any],
    gold_files: list[str],
    known_symbols: set[str] | None = None,
) -> dict[str, Any]:
    return {
        "patch_applied": patch_applied(patch_text),
        "changed_files": changed_files_from_patch(patch_text),
        "wrong_file_edits": wrong_file_edits(patch_text, gold_files),
        "changed_files_missing_from_context": changed_files_missing_from_context(patch_text, packet),
        "nonexistent_symbol_refs": nonexistent_symbol_refs(patch_text, known_symbols=known_symbols),
        "rg_flood_count": rg_flood_count(packet),
    }


def _file_value(value: Any) -> str:
    if isinstance(value, str):
        return normalize_path(value)
    if isinstance(value, dict):
        for key in ("file", "path", "file_path"):
            if value.get(key):
                return normalize_path(str(value[key]))
    return ""
