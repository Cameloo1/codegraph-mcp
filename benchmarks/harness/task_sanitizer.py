from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


EVALUATOR_ONLY_FIELDS = {
    "gold_files",
    "gold_symbols",
    "gold_spans",
    "gold_context_filenames",
    "gold_context_paths",
    "expected_files",
    "expected_symbols",
    "expected_intent",
    "expected_critical_files",
    "expected_critical_symbols",
    "expected_unknowns",
    "expected_risks",
    "expected_validation_steps",
    "expected_proof_status",
    "expected_proof_ladder",
    "expected_plan_facts",
    "expected_tests",
    "forbidden_files",
    "forbidden_symbols",
    "forbidden_spans",
    "forbidden_plan_claims",
    "forbidden_edit_targets",
    "hidden_gold_files",
    "hidden_gold_symbols",
    "oracle_labels",
    "oracle_patch_files",
    "answer_metadata",
    "evaluator_notes",
    "leakage_audit_expected_failures",
}


PROVIDER_VISIBLE_FIELDS = {
    "task_id",
    "task_family",
    "task_text",
    "prompt",
    "task",
    "repo_path",
    "repo_kind",
    "task_type",
    "source",
    "metadata",
    "visible_query_terms",
    "visible_file_hints",
    "visible_symbol_hints",
    "visible_text_hints",
    "visible_config_keys",
    "visible_error_messages",
    "visible_stack_frames",
    "visible_language",
    "visible_repo_metadata",
    "max_output_bytes",
    "max_tool_calls",
    "max_wall_time_ms",
}


VISIBLE_METADATA_KEYS = {
    "repo_name",
    "repository",
    "target_file",
    "dataset_level",
    "materialized_repo",
    "materialized_from_snippets",
    "language",
    "visible_query_terms",
}


VISIBLE_HINT_FIELDS = {
    "visible_file_hints": "visible_file_hint",
    "visible_symbol_hints": "visible_symbol_hint",
    "visible_text_hints": "visible_text_hint",
    "visible_config_keys": "config_key",
    "visible_error_messages": "error_message",
    "visible_stack_frames": "stack_frame",
}


@dataclass(frozen=True)
class QueryTermProvenance:
    term: str
    source: str
    visible_in_prompt: bool
    gold_overlap: bool
    allowed: bool

    def to_dict(self) -> dict[str, Any]:
        return {
            "term": self.term,
            "source": self.source,
            "visible_in_prompt": self.visible_in_prompt,
            "gold_overlap": self.gold_overlap,
            "allowed": self.allowed,
        }


def sanitize_provider_task(task: dict[str, Any]) -> dict[str, Any]:
    """Return the task surface a provider may see.

    The returned object keeps compatibility fields like ``task`` and
    ``query_terms`` but only after deriving them from visible task text or
    explicit visible hint fields.
    """

    prompt = _visible_text(task)
    provenance = extract_visible_query_terms(task)
    visible_terms = [item.term for item in provenance if item.allowed]
    provider_task = {
        key: value
        for key, value in task.items()
        if key in PROVIDER_VISIBLE_FIELDS and key not in EVALUATOR_ONLY_FIELDS
    }
    if prompt:
        provider_task.setdefault("task", prompt)
        provider_task.setdefault("task_text", prompt)
        provider_task.setdefault("prompt", prompt)
    if isinstance(provider_task.get("metadata"), dict):
        provider_task["metadata"] = {
            key: value
            for key, value in provider_task["metadata"].items()
            if key in VISIBLE_METADATA_KEYS and key not in EVALUATOR_ONLY_FIELDS
        }
    provider_task["visible_query_terms"] = visible_terms
    provider_task["query_terms"] = visible_terms
    provider_task["visible_query_term_sources"] = [item.to_dict() for item in provenance]
    provider_task["sanitized_fields_removed"] = sorted(key for key in task if key in EVALUATOR_ONLY_FIELDS)
    provider_task["leakage_audit"] = audit_provider_task(provider_task, source_task=task)
    return provider_task


def extract_visible_query_terms(task: dict[str, Any], *, max_terms: int = 12) -> list[QueryTermProvenance]:
    prompt = _visible_text(task)
    prompt_lower = prompt.lower()
    gold_terms = _gold_terms(task)
    entries: list[QueryTermProvenance] = []

    for term in _terms_from_prompt(prompt):
        entries.append(_provenance(term, "prompt", prompt_lower, gold_terms, explicit_visible=True))

    for term in _iter_existing_visible_query_terms(task):
        entries.append(_provenance(term, "visible_seed", prompt_lower, gold_terms, explicit_visible=True))

    for field, source in VISIBLE_HINT_FIELDS.items():
        for term in _iter_terms(task.get(field)):
            entries.append(_provenance(term, source, prompt_lower, gold_terms, explicit_visible=True))

    deduped: list[QueryTermProvenance] = []
    seen: set[str] = set()
    for entry in entries:
        key = _term_key(entry.term)
        if not key or key in seen:
            continue
        seen.add(key)
        if entry.allowed:
            deduped.append(entry)
        else:
            deduped.append(entry)
        if len([item for item in deduped if item.allowed]) >= max_terms:
            break
    return deduped


def audit_provider_task(provider_task: dict[str, Any], *, source_task: dict[str, Any] | None = None) -> dict[str, Any]:
    forbidden_keys = sorted(key for key in provider_task if key in EVALUATOR_ONLY_FIELDS)
    provenance = provider_task.get("visible_query_term_sources", [])
    provenance_by_term = {
        _term_key(str(item.get("term"))): item
        for item in provenance
        if isinstance(item, dict) and item.get("term")
    }
    disallowed_terms = [
        item
        for item in provenance
        if isinstance(item, dict) and item.get("gold_overlap") and not item.get("allowed")
    ]
    query_terms_value = provider_task.get("query_terms", [])
    query_terms = [str(term) for term in query_terms_value] if isinstance(query_terms_value, list) else []
    source_gold = _gold_terms(source_task or {})
    prompt_lower = _visible_text(source_task or provider_task).lower()
    direct_hidden_gold_terms = []
    for term in query_terms:
        normalized = _term_key(term)
        provenance_item = provenance_by_term.get(normalized, {})
        provenance_allows = bool(provenance_item.get("allowed"))
        if normalized in source_gold and normalized not in prompt_lower and not provenance_allows:
            direct_hidden_gold_terms.append(term)
    passed = not forbidden_keys and not disallowed_terms and not direct_hidden_gold_terms
    return {
        "schema_version": "benchmark_leakage_audit_v1",
        "pass": passed,
        "forbidden_keys_present": forbidden_keys,
        "disallowed_gold_query_terms": disallowed_terms,
        "direct_hidden_gold_query_terms": direct_hidden_gold_terms,
    }


def hidden_gold_query_terms(task: dict[str, Any]) -> list[str]:
    provider_task = sanitize_provider_task(task)
    audit = provider_task.get("leakage_audit", {})
    return list(audit.get("direct_hidden_gold_query_terms", [])) + [
        str(item.get("term")) for item in audit.get("disallowed_gold_query_terms", []) if isinstance(item, dict)
    ]


def _provenance(
    term: str,
    source: str,
    prompt_lower: str,
    gold_terms: set[str],
    *,
    explicit_visible: bool,
) -> QueryTermProvenance:
    clean = _clean_term(term)
    key = _term_key(clean)
    visible_in_prompt = bool(key and key in prompt_lower)
    gold_overlap = key in gold_terms if key else False
    allowed = bool(clean) and (not gold_overlap or visible_in_prompt or explicit_visible)
    return QueryTermProvenance(
        term=clean,
        source=source,
        visible_in_prompt=visible_in_prompt,
        gold_overlap=gold_overlap,
        allowed=allowed,
    )


def _visible_text(task: dict[str, Any]) -> str:
    for key in ("task_text", "prompt", "task"):
        value = task.get(key)
        if isinstance(value, str) and value.strip():
            return value
    return ""


def _terms_from_prompt(text: str) -> list[str]:
    terms: list[str] = []
    for match in re.findall(r"[A-Za-z_][A-Za-z0-9_]{2,}|[\w./\\-]+\.[A-Za-z0-9_]+", text):
        clean = _clean_term(match)
        if clean and len(clean) > 2:
            terms.append(clean)
    return terms


def _iter_existing_visible_query_terms(task: dict[str, Any]) -> list[str]:
    explicit = task.get("visible_query_terms")
    if isinstance(explicit, list):
        return [str(value) for value in explicit if str(value).strip()]
    metadata = task.get("metadata")
    if isinstance(metadata, dict) and metadata.get("visible_query_terms"):
        return [str(value) for value in metadata["visible_query_terms"] if str(value).strip()]
    return []


def _iter_terms(value: Any) -> list[str]:
    if value is None:
        return []
    if isinstance(value, str):
        return [value]
    if isinstance(value, dict):
        return [str(item) for item in value.values() if str(item).strip()]
    if isinstance(value, list):
        return [str(item) for item in value if str(item).strip()]
    return [str(value)]


def _gold_terms(task: dict[str, Any]) -> set[str]:
    terms: set[str] = set()
    for key in (
        "gold_files",
        "gold_symbols",
        "gold_context_filenames",
        "gold_context_paths",
        "expected_files",
        "expected_symbols",
        "expected_critical_files",
        "expected_critical_symbols",
        "hidden_gold_files",
        "hidden_gold_symbols",
        "forbidden_files",
        "forbidden_symbols",
        "forbidden_edit_targets",
        "oracle_patch_files",
    ):
        for value in _iter_terms(task.get(key)):
            clean = _term_key(value)
            if clean:
                terms.add(clean)
            if "/" in value or "\\" in value:
                name = _term_key(Path(value.replace("\\", "/")).name)
                if name:
                    terms.add(name)
    return terms


def _clean_term(value: str) -> str:
    return value.strip(" \t\r\n.,:;()[]{}\"'`")


def _term_key(value: str) -> str:
    return _clean_term(str(value)).replace("\\", "/").lower()
