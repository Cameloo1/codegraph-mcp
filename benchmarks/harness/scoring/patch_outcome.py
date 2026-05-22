from __future__ import annotations

from benchmarks.harness.scoring.context_recall import normalize_path


def changed_files_from_patch(patch_text: str) -> list[str]:
    files: list[str] = []
    for line in patch_text.splitlines():
        if line.startswith("+++ b/"):
            path = normalize_path(line.removeprefix("+++ b/"))
            if path != "/dev/null" and path not in files:
                files.append(path)
    return files


def wrong_file_edits(patch_text: str, gold_files: list[str]) -> int:
    allowed = {normalize_path(path) for path in gold_files}
    if not allowed:
        return 0
    return sum(1 for path in changed_files_from_patch(patch_text) if normalize_path(path) not in allowed)


def patch_applied(patch_text: str | None) -> bool:
    return bool(patch_text and "+++ b/" in patch_text and "--- a/" in patch_text)

