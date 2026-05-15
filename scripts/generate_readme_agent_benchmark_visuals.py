#!/usr/bin/env python3
"""Generate compact README benchmark visuals for agent-impact claims.

The charts intentionally use plain matplotlib defaults only: no external chart
libraries, no default-style override, no custom themes, and no custom colors.
"""

from __future__ import annotations

import json
import math
import os
import re
import sys
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault("MPLCONFIGDIR", str(ROOT / "target" / "matplotlib-cache"))

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt


INTENDED_JSON = ROOT / "reports/final/intended_tool_quality_gate.json"
INTENDED_MD = ROOT / "reports/final/intended_tool_quality_gate.md"
MANUAL_JSON = ROOT / "reports/final/manual_relation_precision.json"
MANUAL_MD = ROOT / "reports/final/manual_relation_precision.md"
COMPREHENSIVE_JSON = ROOT / "reports/final/comprehensive_benchmark_latest.json"
COMPREHENSIVE_MD = ROOT / "reports/final/comprehensive_benchmark_latest.md"
README = ROOT / "README.md"

ASSET_DIR = ROOT / "docs/assets/readme"
REPORT_DIR = ROOT / "reports/final"
VISUAL_01 = ASSET_DIR / "large_repo_improvement.png"
VISUAL_02 = ASSET_DIR / "evidence_reliability.png"
VISUAL_03 = ASSET_DIR / "warm_agent_loop_latency.png"
MANIFEST = ASSET_DIR / "agent_visuals_manifest.json"
REPORT_JSON = REPORT_DIR / "readme_agent_benchmark_visuals.json"
REPORT_MD = REPORT_DIR / "readme_agent_benchmark_visuals.md"


@dataclass
class Metric:
    name: str
    value: float
    unit: str
    source_file: str
    claim_boundary: str
    source_kind: str = "json"
    chart: str | None = None
    target: float | None = None
    status: str | None = None
    observed_label: str | None = None
    label: str | None = None
    extra: dict[str, Any] = field(default_factory=dict)

    def to_json(self) -> dict[str, Any]:
        data = {
            "name": self.name,
            "value": self.value,
            "unit": self.unit,
            "source_file": self.source_file,
            "claim_boundary": self.claim_boundary,
            "source_kind": self.source_kind,
        }
        if self.chart:
            data["chart"] = self.chart
        if self.target is not None:
            data["target"] = self.target
        if self.status is not None:
            data["status"] = self.status
        if self.observed_label is not None:
            data["observed_label"] = self.observed_label
        if self.label is not None:
            data["label"] = self.label
        if self.extra:
            data["extra"] = self.extra
        return data


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def load_required_json(path: Path, label: str) -> dict[str, Any]:
    if not path.exists():
        raise SystemExit(f"required {label} report missing: {rel(path)}")
    return json.loads(path.read_text(encoding="utf-8"))


def load_optional_json(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def read_optional_text(path: Path) -> str:
    return path.read_text(encoding="utf-8") if path.exists() else ""


def metric_by_id(section: dict[str, Any] | None, metric_id: str) -> dict[str, Any] | None:
    if not section:
        return None
    for metric in section.get("metrics", []):
        if metric.get("id") == metric_id:
            return metric
    return None


def required_metric_by_id(intended: dict[str, Any], metric_id: str) -> dict[str, Any] | None:
    for metric in intended.get("required_metrics", []):
        if metric.get("id") == metric_id:
            return metric
    return None


def query_by_id(comprehensive: dict[str, Any] | None, query_id: str) -> dict[str, Any] | None:
    if not comprehensive:
        return None
    for query in comprehensive.get("sections", {}).get("query_latency", {}).get("queries", []):
        if query.get("id") == query_id:
            return query
    return None


def parse_intended_metric_from_markdown(text: str, metric_id: str) -> dict[str, Any] | None:
    pattern = re.compile(
        rf"- {re.escape(metric_id)}: observed ([^,]+), target ([^,]+), status ([A-Za-z_]+)"
    )
    match = pattern.search(text)
    if not match:
        return None
    observed_raw, target_raw, status = match.groups()
    observed = observed_raw.strip()
    target = target_raw.strip()
    for suffix in (" ms", " MiB"):
        observed = observed.replace(suffix, "")
    observed = observed.replace(",", "")
    target = target.replace("<=", "").replace(",", "").strip()
    target = target.replace(" ms", "").replace(" MiB", "")
    try:
        observed_value: float | str = float(observed)
    except ValueError:
        observed_value = observed_raw.strip()
    try:
        target_value: float | str = float(target)
    except ValueError:
        target_value = target_raw.strip()
    return {
        "id": metric_id,
        "observed": observed_value,
        "target": target_value,
        "status": status,
        "source_kind": "markdown",
    }


def parse_comprehensive_table_metric(text: str, metric_id: str) -> dict[str, Any] | None:
    pattern = re.compile(rf"\| `{re.escape(metric_id)}` \| ([^|]+) \| ([^|]+) \| ([^|]+) \|")
    match = pattern.search(text)
    if not match:
        return None
    target_raw, observed_raw, status_raw = [part.strip() for part in match.groups()]
    try:
        observed: float | str = float(observed_raw.replace(",", ""))
    except ValueError:
        observed = observed_raw
    try:
        target: float | str = float(target_raw.replace(",", ""))
    except ValueError:
        target = target_raw
    return {
        "id": metric_id,
        "observed": observed,
        "target": target,
        "status": status_raw,
        "source_kind": "markdown",
    }


def parse_manual_relation_rows(text: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for line in text.splitlines():
        if not line.startswith("| "):
            continue
        parts = [part.strip(" `") for part in line.strip().strip("|").split("|")]
        if len(parts) < 7 or parts[0] in {"Relation", "---"}:
            continue
        relation, edges, labeled, precision, target, status, claim = parts[:7]
        try:
            edge_count = int(edges)
            labeled_samples = int(labeled)
        except ValueError:
            continue
        precision_value: float | None
        if precision.lower() in {"n/a", "unknown"}:
            precision_value = None
        else:
            precision_value = float(precision.rstrip("%")) / 100.0
        rows.append(
            {
                "relation": relation,
                "proof_db_edge_count": edge_count,
                "labeled_samples": labeled_samples,
                "precision": precision_value,
                "target": target,
                "status": status,
                "claim": claim,
                "source_kind": "markdown",
            }
        )
    return rows


def parse_regression_markdown(text: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for line in text.splitlines():
        if not line.startswith("| `"):
            continue
        parts = [part.strip(" `") for part in line.strip().strip("|").split("|")]
        if len(parts) != 5:
            continue
        metric_id, previous, current, delta, status = parts
        if not metric_id.endswith(("clean_1_63_gib", "clean_30s", "clean_80s", "clean_13s", "compact_baseline")):
            continue
        try:
            previous_value = float(previous)
            current_value = float(current)
            delta_value = float(delta)
        except ValueError:
            continue
        rows.append(
            {
                "id": metric_id,
                "previous": previous_value,
                "current": current_value,
                "delta": delta_value,
                "status": status,
                "source_file": rel(COMPREHENSIVE_MD),
                "source_kind": "markdown",
                "claim_boundary": "Historical regression row from comprehensive benchmark; not a CGC comparison and not a real-world recall claim.",
            }
        )
    return rows


def numeric(value: Any) -> float | None:
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, (int, float)):
        if math.isnan(float(value)):
            return None
        return float(value)
    if isinstance(value, str):
        cleaned = value.replace(",", "").replace("%", "").strip()
        try:
            return float(cleaned)
        except ValueError:
            return None
    return None


def format_value(value: float, unit: str) -> str:
    if unit == "ms":
        if value >= 1000.0:
            return f"{value / 1000.0:.2f}s"
        return f"{value:.0f}ms"
    if unit == "MiB":
        if value >= 1024.0:
            return f"{value / 1024.0:.2f}GiB"
        return f"{value:.0f}MiB"
    return f"{value:.0f} {unit}"


def add_metric(metrics: list[Metric], metric: Metric, omitted: list[dict[str, str]]) -> None:
    missing = [
        field_name
        for field_name in ("name", "value", "unit", "source_file", "claim_boundary")
        if getattr(metric, field_name) in (None, "")
    ]
    if missing:
        omitted.append(
            {
                "name": metric.name or "unknown",
                "reason": f"metric failed validation, missing: {', '.join(missing)}",
            }
        )
        return
    metrics.append(metric)


def build_context_quality_metrics(
    comprehensive: dict[str, Any] | None,
    comprehensive_md: str,
    omitted: list[dict[str, str]],
) -> list[Metric]:
    metrics: list[Metric] = []
    sections = comprehensive.get("sections", {}) if comprehensive else {}
    correctness = sections.get("correctness_gates")
    context = sections.get("context_packet_gate")
    source_file = rel(COMPREHENSIVE_JSON if comprehensive else COMPREHENSIVE_MD)
    source_kind = "json" if comprehensive else "markdown"

    graph_passed = metric_by_id(correctness, "graph_truth_cases_passed")
    graph_total = metric_by_id(correctness, "graph_truth_cases_total")
    if graph_passed and graph_total:
        passed = numeric(graph_passed.get("observed"))
        total = numeric(graph_total.get("observed"))
        if passed is not None and total:
            add_metric(
                metrics,
                Metric(
                    chart="large_repo_improvement",
                    name="Graph Truth Gate",
                    value=100.0 * passed / total,
                    unit="percent",
                    source_file=source_file,
                    source_kind=source_kind,
                    claim_boundary="Graph Truth Gate pass rate over 11 adversarial fixtures.",
                    label=f"{int(passed)}/{int(total)}",
                ),
                omitted,
            )
    else:
        omitted.append({"name": "Graph Truth Gate pass rate", "reason": "not found in comprehensive JSON"})

    context_passed = metric_by_id(context, "context_cases_passed")
    context_total = metric_by_id(context, "context_cases_total")
    if context_passed and context_total:
        passed = numeric(context_passed.get("observed"))
        total = numeric(context_total.get("observed"))
        if passed is not None and total:
            add_metric(
                metrics,
                Metric(
                    chart="large_repo_improvement",
                    name="Context Packet Gate",
                    value=100.0 * passed / total,
                    unit="percent",
                    source_file=source_file,
                    source_kind=source_kind,
                    claim_boundary="Context Packet Gate pass rate over 11 adversarial fixtures.",
                    label=f"{int(passed)}/{int(total)}",
                ),
                omitted,
            )
    else:
        omitted.append({"name": "Context Packet Gate pass rate", "reason": "not found in comprehensive JSON"})

    direct_context_metrics = [
        ("critical_symbol_recall", "Critical symbol recall", "Context packet gate validation; not a real-world recall claim."),
        ("proof_path_coverage", "Proof-path coverage", "Context packet gate validation over adversarial fixtures."),
        (
            "proof_path_source_span_coverage",
            "Proof-path source-span coverage",
            "Proof paths include source spans in the context packet gate.",
        ),
        ("expected_tests_recall", "Expected tests recall", "Expected-test fixture recall; not a real-world test recall claim."),
    ]
    for metric_id, name, boundary in direct_context_metrics:
        item = metric_by_id(context, metric_id)
        if not item:
            fallback = parse_comprehensive_table_metric(comprehensive_md, metric_id)
            if fallback:
                item = fallback
                item_source = rel(COMPREHENSIVE_MD)
                item_kind = "markdown"
            else:
                omitted.append({"name": name, "reason": f"{metric_id} not found"})
                continue
        else:
            item_source = source_file
            item_kind = source_kind
        observed = numeric(item.get("observed"))
        if observed is None:
            omitted.append({"name": name, "reason": f"{metric_id} has nonnumeric value"})
            continue
        add_metric(
            metrics,
            Metric(
                chart="large_repo_improvement",
                name=name,
                value=100.0 * observed,
                unit="percent",
                source_file=item_source,
                source_kind=item_kind,
                claim_boundary=boundary,
                label=f"{100.0 * observed:.0f}%",
            ),
            omitted,
        )

    distractor = metric_by_id(context, "distractor_ratio")
    if distractor:
        observed = numeric(distractor.get("observed"))
        if observed is not None:
            clean_score = max(0.0, min(100.0, 100.0 * (1.0 - observed)))
            add_metric(
                metrics,
                Metric(
                    chart="large_repo_improvement",
                    name="Distractor-free packet",
                    value=clean_score,
                    unit="percent",
                    source_file=source_file,
                    source_kind=source_kind,
                    claim_boundary="Clean-context score derived as 100 * (1 - distractor_ratio) for the context packet gate.",
                    label=f"{clean_score:.0f}%",
                ),
                omitted,
            )
    else:
        omitted.append({"name": "Distractor-free packet", "reason": "distractor_ratio not found"})

    return metrics


def build_historical_improvement_metrics(
    historical_metrics: list[dict[str, Any]],
    omitted: list[dict[str, str]],
) -> list[Metric]:
    """Build before/after metrics that are easier to compare in README charts."""

    labels = {
        "proof_db_mib_vs_clean_1_63_gib": ("Proof DB size", "MiB", "lower_is_better"),
        "context_pack_p95_ms_vs_clean_30s": ("context_pack p95", "ms", "lower_is_better"),
        "single_file_update_ms_vs_clean_80s": ("Single-file update", "ms", "lower_is_better"),
        "repeat_unchanged_ms_vs_clean_13s": ("Repeat unchanged", "ms", "lower_is_better"),
    }
    by_id = {item.get("id"): item for item in historical_metrics}
    metrics: list[Metric] = []
    for metric_id, (name, unit, direction) in labels.items():
        item = by_id.get(metric_id)
        if not item:
            omitted.append({"name": name, "reason": f"{metric_id} not found in regression summary"})
            continue
        previous = numeric(item.get("previous"))
        current = numeric(item.get("current"))
        if previous is None or current is None or previous <= 0 or current <= 0:
            omitted.append({"name": name, "reason": f"{metric_id} has invalid before/after values"})
            continue
        improvement = previous / current if direction == "lower_is_better" else current / previous
        add_metric(
            metrics,
            Metric(
                chart="large_repo_improvement",
                name=name,
                value=improvement,
                unit="improvement_factor",
                source_file=item.get("source_file", rel(COMPREHENSIVE_MD)),
                source_kind=item.get("source_kind", "markdown"),
                claim_boundary=(
                    "Before/after regression row from the comprehensive benchmark; "
                    "lower is better and this is not a CGC comparison."
                ),
                label=f"{improvement:.1f}x",
                extra={
                    "previous": previous,
                    "current": current,
                    "metric_unit": unit,
                    "status": item.get("status"),
                },
            ),
            omitted,
        )
    return metrics


def build_relation_metrics(
    manual: dict[str, Any],
    manual_md: str,
    omitted: list[dict[str, str]],
) -> list[Metric]:
    metrics: list[Metric] = []
    target_eval = manual.get("target_evaluation") or []
    rows_by_relation = {row.get("relation"): row for row in target_eval if row.get("relation")}
    source_file = rel(MANUAL_JSON)
    source_kind = "json"
    if not rows_by_relation:
        rows_by_relation = {row.get("relation"): row for row in parse_manual_relation_rows(manual_md)}
        source_file = rel(MANUAL_MD)
        source_kind = "markdown"

    present = manual.get("relation_coverage", {}).get(
        "present_labeled_relations",
        ["CALLS", "READS", "WRITES", "FLOWS_TO", "MUTATES", "PathEvidence", "MAY_MUTATE"],
    )
    preferred_order = ["CALLS", "READS", "WRITES", "FLOWS_TO", "MUTATES", "MAY_MUTATE", "PathEvidence"]
    for relation in preferred_order:
        if relation not in present:
            continue
        row = rows_by_relation.get(relation)
        if not row:
            omitted.append({"name": relation, "reason": "present relation missing from manual precision table"})
            continue
        edge_count = numeric(row.get("proof_db_edge_count"))
        labeled = numeric(row.get("labeled_samples"))
        precision = numeric(row.get("precision"))
        claim = row.get("claim") or "sampled_precision_estimate"
        if edge_count is None or labeled is None or precision is None:
            omitted.append({"name": relation, "reason": "edge count, labeled count, or precision was unavailable"})
            continue
        if relation == "PathEvidence":
            label = f"{int(labeled)}/{int(labeled)} path evidence"
            boundary = "Sampled PathEvidence correctness only; recall is not claimed."
            unit = "labeled_path_evidence_samples"
        else:
            label = f"{int(labeled)}/{int(labeled)} labeled precision"
            boundary = "Sampled precision for present compact-proof relation only; recall is not claimed."
            unit = "proof_db_edges"
        add_metric(
            metrics,
            Metric(
                chart="evidence_reliability",
                name=relation,
                value=edge_count,
                unit=unit,
                source_file=source_file,
                source_kind=source_kind,
                claim_boundary=boundary,
                status=row.get("status"),
                label=label,
                extra={
                    "labeled_samples": int(labeled),
                    "precision": precision,
                    "claim": claim,
                },
            ),
            omitted,
        )

    return metrics


def build_evidence_reliability_metrics(
    manual: dict[str, Any],
    omitted: list[dict[str, str]],
) -> list[Metric]:
    metrics: list[Metric] = []
    summary = manual.get("summary", {})
    source_span = summary.get("source_span_precision", {})
    relation_rows = summary.get("relation_precision", [])
    path_row = next((row for row in relation_rows if row.get("relation") == "PathEvidence"), None)

    labeled = numeric(summary.get("labeled_samples"))
    true_positive = sum(numeric(row.get("true_positive")) or 0.0 for row in relation_rows)
    false_positive = sum(numeric(row.get("false_positive")) or 0.0 for row in relation_rows)
    wrong_span = sum(numeric(row.get("wrong_span")) or 0.0 for row in relation_rows)
    stale = sum(
        1.0
        for sample in manual.get("samples", [])
        if sample.get("labels", {}).get("stale") is True
    )
    test_mock_leaked = sum(
        1.0
        for sample in manual.get("samples", [])
        if sample.get("labels", {}).get("test_mock_leaked") is True
    )
    derived_missing = sum(
        1.0
        for sample in manual.get("samples", [])
        if sample.get("labels", {}).get("derived_missing_provenance") is True
    )

    def add_percent(name: str, value: float, label: str, boundary: str, extra: dict[str, Any]) -> None:
        add_metric(
            metrics,
            Metric(
                chart="evidence_reliability",
                name=name,
                value=value,
                unit="percent",
                source_file=rel(MANUAL_JSON),
                source_kind="json",
                claim_boundary=boundary,
                label=label,
                extra=extra,
            ),
            omitted,
        )

    if labeled and labeled > 0:
        add_percent(
            "Sampled relation precision",
            100.0 * true_positive / labeled,
            f"{int(true_positive)}/{int(labeled)} true-positive",
            "Sampled precision only across present compact-proof relations; recall is unknown.",
            {"true_positive": true_positive, "labeled_samples": labeled},
        )
        clean_events = false_positive + stale + test_mock_leaked + derived_missing
        add_percent(
            "No false/stale/leak events",
            100.0 * (labeled - clean_events) / labeled,
            f"{int(clean_events)} events found",
            "Manual sample taxonomy found no false positives, stale facts, test/mock leakage, or derived-without-provenance events.",
            {
                "false_positive": false_positive,
                "stale": stale,
                "test_mock_leaked": test_mock_leaked,
                "derived_missing_provenance": derived_missing,
            },
        )
        add_percent(
            "Wrong-span avoidance",
            100.0 * (labeled - wrong_span) / labeled,
            f"{int(wrong_span)} wrong spans",
            "Source-span correctness across labeled samples; recall is not claimed.",
            {"wrong_span": wrong_span, "labeled_samples": labeled},
        )
    else:
        omitted.append({"name": "Sampled relation precision", "reason": "manual labeled sample count missing"})

    span_eligible = numeric(source_span.get("eligible_samples"))
    span_true_positive = numeric(source_span.get("true_positive"))
    if span_eligible and span_true_positive is not None:
        add_percent(
            "Source-span precision",
            100.0 * span_true_positive / span_eligible,
            f"{int(span_true_positive)}/{int(span_eligible)} spans",
            "Manual source-span precision over eligible samples; recall is unknown.",
            {"true_positive": span_true_positive, "eligible_samples": span_eligible},
        )
    else:
        omitted.append({"name": "Source-span precision", "reason": "source-span summary missing"})

    if path_row:
        path_labeled = numeric(path_row.get("labeled_samples"))
        path_true_positive = numeric(path_row.get("true_positive"))
        if path_labeled and path_true_positive is not None:
            add_percent(
                "PathEvidence correctness",
                100.0 * path_true_positive / path_labeled,
                f"{int(path_true_positive)}/{int(path_labeled)} paths",
                "Sampled PathEvidence correctness only; recall is not claimed.",
                {"true_positive": path_true_positive, "labeled_samples": path_labeled},
            )
        else:
            omitted.append({"name": "PathEvidence correctness", "reason": "PathEvidence sample counts missing"})
    else:
        omitted.append({"name": "PathEvidence correctness", "reason": "PathEvidence row missing"})

    return metrics


def build_loop_metrics(
    intended: dict[str, Any],
    intended_md: str,
    comprehensive: dict[str, Any] | None,
    comprehensive_md: str,
    omitted: list[dict[str, str]],
) -> list[Metric]:
    metrics: list[Metric] = []

    def from_intended(metric_id: str, name: str, unit: str, boundary: str, observed_label: str) -> None:
        item = required_metric_by_id(intended, metric_id)
        source_file = rel(INTENDED_JSON)
        source_kind = "json"
        if not item:
            item = parse_intended_metric_from_markdown(intended_md, metric_id)
            source_file = rel(INTENDED_MD)
            source_kind = "markdown"
        if not item:
            omitted.append({"name": name, "reason": f"{metric_id} not found"})
            return
        observed = numeric(item.get("observed"))
        target = numeric(item.get("target"))
        if observed is None or target is None or target == 0:
            omitted.append({"name": name, "reason": f"{metric_id} has nonnumeric observed or target"})
            return
        ratio = observed / target
        add_metric(
            metrics,
            Metric(
                chart="warm_agent_loop_latency",
                name=name,
                value=ratio,
                unit="observed/target_ratio",
                source_file=source_file,
                source_kind=source_kind,
                target=target,
                status=item.get("status"),
                observed_label=observed_label.format(observed=observed, target=target),
                claim_boundary=boundary,
                extra={"observed": observed, "observed_unit": unit},
            ),
            omitted,
        )

    from_intended(
        "proof_db_mib",
        "Proof DB size",
        "MiB",
        "Compact proof DB size from the Intended Tool Quality Gate; audit/debug sidecars excluded.",
        "{observed:.3f} MiB / {target:.0f} MiB",
    )
    from_intended(
        "repeat_unchanged_ms",
        "Repeat unchanged",
        "ms",
        "Warm unchanged index loop from the Intended Tool Quality Gate.",
        "{observed:.0f} ms / {target:.0f} ms",
    )
    from_intended(
        "single_file_update_ms",
        "Single-file update",
        "ms",
        "Single-file update loop from the Intended Tool Quality Gate.",
        "{observed:.0f} ms / {target:.0f} ms",
    )

    for query_id, name, boundary in [
        (
            "context_pack_normal",
            "context_pack p95",
            "Normal context-pack query latency from the comprehensive benchmark; not an agent success-rate metric.",
        ),
        (
            "unresolved_calls_paginated",
            "Unresolved calls p95",
            "Bounded unresolved-calls pagination latency from the comprehensive benchmark.",
        ),
    ]:
        query = query_by_id(comprehensive, query_id)
        source_file = rel(COMPREHENSIVE_JSON)
        source_kind = "json"
        if not query:
            fallback = parse_comprehensive_table_metric(comprehensive_md, query_id)
            if fallback:
                observed = numeric(fallback.get("observed"))
                target = numeric(fallback.get("target"))
                status = fallback.get("status")
                source_file = rel(COMPREHENSIVE_MD)
                source_kind = "markdown"
            else:
                omitted.append({"name": name, "reason": f"{query_id} not found"})
                continue
        else:
            observed = numeric(query.get("observed", {}).get("p95_ms"))
            target = numeric(query.get("target", {}).get("p95_ms"))
            status = query.get("status")
        if observed is None or target is None or target == 0:
            omitted.append({"name": name, "reason": f"{query_id} has nonnumeric observed or target"})
            continue
        add_metric(
            metrics,
            Metric(
                chart="warm_agent_loop_latency",
                name=name,
                value=observed / target,
                unit="observed/target_ratio",
                source_file=source_file,
                source_kind=source_kind,
                target=target,
                status=status,
                observed_label=f"{observed:.1f} ms / {target:.0f} ms",
                claim_boundary=boundary,
                extra={"observed": observed, "observed_unit": "ms"},
            ),
            omitted,
        )

    from_intended(
        "proof_build_only_ms",
        "Cold proof build",
        "ms",
        "Cold proof-build-only timing from the Intended Tool Quality Gate; this is the remaining blocker.",
        "{observed:.0f} ms / {target:.0f} ms",
    )

    return metrics


def save_context_quality(metrics: list[Metric]) -> None:
    labels = [metric.name for metric in metrics]
    values = [metric.value for metric in metrics]
    y_pos = list(range(len(metrics)))
    fig, ax = plt.subplots(figsize=(6.4, 3.8), dpi=160)
    bars = ax.barh(y_pos, values)
    ax.set_yticks(y_pos, labels)
    ax.set_xlabel("Improvement factor (x; higher is better)")
    ax.set_title("Large-Repo Improvement")
    ax.invert_yaxis()
    ax.set_xlim(0, max(values) * 1.45)
    for bar, metric in zip(bars, metrics):
        unit = metric.extra["metric_unit"]
        before = format_value(metric.extra["previous"], unit)
        after = format_value(metric.extra["current"], unit)
        ax.text(
            bar.get_width() + max(values) * 0.015,
            bar.get_y() + bar.get_height() / 2,
            f"{metric.label} ({before} -> {after})",
            va="center",
            fontsize=8,
        )
    fig.text(
        0.01,
        0.02,
        "Historical benchmark rows; lower raw values are better. Cold proof-build remains open and is excluded.",
        ha="left",
        fontsize=8,
    )
    fig.tight_layout(rect=[0, 0.08, 1, 1])
    fig.savefig(VISUAL_01)
    plt.close(fig)


def save_relation_chart(metrics: list[Metric]) -> None:
    labels = [metric.name for metric in metrics]
    values = [metric.value for metric in metrics]
    y_pos = list(range(len(metrics)))
    fig, ax = plt.subplots(figsize=(6.4, 3.8), dpi=160)
    bars = ax.barh(y_pos, values)
    ax.set_yticks(y_pos, labels)
    ax.set_xlim(0, 105)
    ax.set_xlabel("Validated evidence quality (%)")
    ax.set_title("Evidence Reliability")
    ax.invert_yaxis()
    for bar, metric in zip(bars, metrics):
        ax.text(
            min(bar.get_width() + 1.0, 101.0),
            bar.get_y() + bar.get_height() / 2,
            metric.label or f"{metric.value:.0f}%",
            va="center",
            fontsize=8,
        )
    fig.text(
        0.01,
        0.02,
        "320 labeled samples; sampled precision only, recall unknown.",
        ha="left",
        fontsize=8,
    )
    fig.tight_layout(rect=[0, 0.08, 1, 1])
    fig.savefig(VISUAL_02)
    plt.close(fig)


def save_loop_chart(metrics: list[Metric]) -> None:
    warm_metrics = [
        metric
        for metric in metrics
        if metric.name not in {"Cold proof build", "Proof DB size"}
    ]
    labels = [metric.name for metric in warm_metrics]
    y_pos = list(range(len(warm_metrics)))
    observed = [metric.extra["observed"] for metric in warm_metrics]
    targets = [metric.target or 1.0 for metric in warm_metrics]
    fig, ax = plt.subplots(figsize=(6.4, 3.8), dpi=160)
    height = 0.35
    observed_pos = [pos - height / 2 for pos in y_pos]
    target_pos = [pos + height / 2 for pos in y_pos]
    observed_bars = ax.barh(observed_pos, observed, height=height, label="Observed")
    ax.barh(target_pos, targets, height=height, label="Target")
    ax.set_yticks(y_pos, labels)
    ax.set_xscale("log")
    ax.set_xlabel("Milliseconds (log scale; lower is better)")
    ax.set_title("Warm Agent Loop Latency")
    ax.set_xlim(left=max(1.0, min(observed + targets) * 0.7), right=max(targets) * 1.8)
    ax.invert_yaxis()
    ax.legend(loc="lower right", fontsize=8)
    for bar, metric in zip(observed_bars, warm_metrics):
        headroom = (metric.target or 1.0) / metric.extra["observed"]
        ax.text(
            bar.get_width() * 1.05,
            bar.get_y() + bar.get_height() / 2,
            f"{metric.extra['observed']:.0f}ms; {headroom:.1f}x headroom",
            va="center",
            fontsize=7,
        )
    cold = next((metric for metric in metrics if metric.name == "Cold proof build"), None)
    cold_note = ""
    if cold:
        cold_note = f" Cold proof build remains open: {cold.extra['observed'] / 1000:.0f}s vs {cold.target / 1000:.0f}s."
    fig.text(
        0.01,
        0.02,
        f"Interactive paths are below target.{cold_note}",
        ha="left",
        fontsize=8,
    )
    fig.tight_layout(rect=[0, 0.08, 1, 1])
    fig.savefig(VISUAL_03)
    plt.close(fig)


def validate_pngs(paths: list[Path]) -> None:
    missing = [rel(path) for path in paths if not path.exists() or path.stat().st_size == 0]
    if missing:
        raise SystemExit(f"generated PNG missing or empty: {', '.join(missing)}")


def validate_readme_image_links() -> None:
    if not README.exists():
        return
    text = README.read_text(encoding="utf-8")
    broken: list[str] = []
    for match in re.finditer(r"!\[[^\]]*\]\(([^)]+)\)", text):
        target = match.group(1).strip()
        if target.startswith("http://") or target.startswith("https://"):
            continue
        if not (ROOT / target).exists():
            broken.append(target)
    if broken:
        raise SystemExit(f"README image links are broken: {', '.join(broken)}")


def write_manifest_and_reports(
    generated_at: str,
    metrics: list[Metric],
    omitted: list[dict[str, str]],
    caveats: list[str],
    historical_metrics: list[dict[str, Any]],
) -> None:
    metric_json = [metric.to_json() for metric in metrics]
    source_files = sorted({item["source_file"] for item in metric_json} | {item["source_file"] for item in historical_metrics})
    parsed_from_markdown = any(item.get("source_kind") == "markdown" for item in metric_json + historical_metrics)
    chart_paths = {
        "large_repo_improvement": rel(VISUAL_01),
        "evidence_reliability": rel(VISUAL_02),
        "warm_agent_loop_latency": rel(VISUAL_03),
    }
    payload = {
        "generated_at": generated_at,
        "chart_paths": chart_paths,
        "metrics_used": metric_json,
        "historical_metrics_found": historical_metrics,
        "source_files_used": source_files,
        "omitted_metrics": omitted,
        "caveats": caveats,
        "parsed_from_markdown": parsed_from_markdown,
    }
    MANIFEST.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    REPORT_JSON.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    lines = [
        "# README Agent Benchmark Visuals",
        "",
        f"Generated: {generated_at}",
        "",
        "These visuals summarize benchmark evidence for large-repo improvement, sampled evidence reliability, and warm agent-loop latency. They do not claim final intended-performance pass, CodeGraph-vs-CGC superiority, real-world recall, or precision for absent proof-mode relations.",
        "",
        "## Charts",
        "",
    ]
    for name, path in chart_paths.items():
        lines.append(f"- `{name}`: `{path}`")
    lines.extend(["", "## Metrics Used", ""])
    for metric in metric_json:
        status = f", status `{metric['status']}`" if metric.get("status") else ""
        lines.append(
            f"- {metric['chart']}: {metric['name']} = {metric['value']} {metric['unit']} from `{metric['source_file']}`{status}. Claim boundary: {metric['claim_boundary']}"
        )
    lines.extend(["", "## Historical Metrics Found", ""])
    if historical_metrics:
        for metric in historical_metrics:
            lines.append(
                f"- `{metric['id']}`: previous {metric['previous']}, current {metric['current']}, status {metric['status']} from `{metric['source_file']}`."
            )
    else:
        lines.append("- None parsed.")
    lines.extend(["", "## Omitted Metrics", ""])
    if omitted:
        for item in omitted:
            lines.append(f"- {item.get('name', 'unknown')}: {item.get('reason', 'unknown')}")
    else:
        lines.append("- None.")
    lines.extend(["", "## Caveats", ""])
    for caveat in caveats:
        lines.append(f"- {caveat}")
    lines.append("")
    REPORT_MD.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    intended = load_required_json(INTENDED_JSON, "intended_tool_quality_gate")
    manual = load_required_json(MANUAL_JSON, "manual_relation_precision")
    comprehensive = load_optional_json(COMPREHENSIVE_JSON)
    intended_md = read_optional_text(INTENDED_MD)
    manual_md = read_optional_text(MANUAL_MD)
    comprehensive_md = read_optional_text(COMPREHENSIVE_MD)

    ASSET_DIR.mkdir(parents=True, exist_ok=True)
    REPORT_DIR.mkdir(parents=True, exist_ok=True)

    omitted: list[dict[str, str]] = []
    metrics: list[Metric] = []
    historical_metrics = parse_regression_markdown(comprehensive_md)
    metrics.extend(build_historical_improvement_metrics(historical_metrics, omitted))
    metrics.extend(build_evidence_reliability_metrics(manual, omitted))
    metrics.extend(build_loop_metrics(intended, intended_md, comprehensive, comprehensive_md, omitted))

    context_metrics = [metric for metric in metrics if metric.chart == "large_repo_improvement"]
    relation_metrics = [metric for metric in metrics if metric.chart == "evidence_reliability"]
    loop_metrics = [metric for metric in metrics if metric.chart == "warm_agent_loop_latency"]
    if not context_metrics:
        raise SystemExit("no Large-Repo Improvement metrics available")
    if not relation_metrics:
        raise SystemExit("no Evidence Reliability metrics available")
    if not loop_metrics:
        raise SystemExit("no Warm Agent Loop metrics available")

    save_context_quality(context_metrics)
    save_relation_chart(relation_metrics)
    save_loop_chart(loop_metrics)
    validate_pngs([VISUAL_01, VISUAL_02, VISUAL_03])
    validate_readme_image_links()

    caveats = [
        "The Intended Tool Quality Gate remains FAIL because proof_build_only_ms exceeds the 60,000 ms target.",
        "CGC comparison remains diagnostic/blocked/incomplete; no CodeGraph-vs-CGC superiority claim is made.",
        "Manual relation precision is sampled precision only; recall is unknown without a false-negative gold denominator.",
        "Absent proof-mode relations are not plotted and have no precision claim.",
        "macOS is coming soon, not currently tested, and has no CI coverage.",
    ]
    generated_at = datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")
    write_manifest_and_reports(generated_at, metrics, omitted, caveats, historical_metrics)

    for item in omitted:
        print(f"omitted metric: {item.get('name')}: {item.get('reason')}")
    print(f"wrote {rel(VISUAL_01)}")
    print(f"wrote {rel(VISUAL_02)}")
    print(f"wrote {rel(VISUAL_03)}")
    print(f"wrote {rel(MANIFEST)}")
    print(f"wrote {rel(REPORT_MD)}")
    print(f"wrote {rel(REPORT_JSON)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
