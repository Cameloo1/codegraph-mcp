#!/usr/bin/env python3
"""Generate README readiness visuals for agent-benchmark positioning.

The output is intentionally front-facing but claim-bounded. The roadmap score is
an internal readiness model, not an official SWE-bench, RepoBench,
CrossCodeEval, CGC, or rg comparison.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault("MPLCONFIGDIR", str(ROOT / "target" / "matplotlib-cache"))

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np


ASSET_DIR = ROOT / "docs/assets/readme"
README = ROOT / "README.md"
BENCHMARK_FULL_RUN_JSON = ROOT / "reports/final/benchmark_full_run_scores.json"
MANUAL_PRECISION_JSON = ROOT / "reports/final/manual_relation_precision.json"
SETUP_JSON = ROOT / "reports/audit/benchmark_setup_final_unblock.json"
PINNED_SOURCES_JSON = ROOT / "benchmarks/upstream/pinned_sources.json"

READINESS_CHART = ASSET_DIR / "mvp4_readiness_over_time.png"
RETRIEVAL_CHART = ASSET_DIR / "retrieval_quality_by_track.png"
EVIDENCE_CHART = ASSET_DIR / "evidence_safety.png"
SWEBENCH_CHART = ASSET_DIR / "swebench_readiness_ladder.png"
MANIFEST = ASSET_DIR / "agent_readiness_manifest.json"

BG = "#070d18"
PANEL = "#0b1320"
GRID = "#243044"
TEXT = "#e8eef8"
MUTED = "#9aa7bb"
WHITE = "#f5f7fb"
GREY = "#8d95a3"
BLUE = "#56b4e9"
GREEN = "#64d387"
YELLOW = "#f2c94c"
PURPLE = "#c084e8"
RED = "#f87171"
DARK_BAR = "#3c4659"


@dataclass(frozen=True)
class RoadmapPoint:
    milestone: str
    score: float
    note: str
    verified_surfaces: int
    scope_denominator: float


ROADMAP_POINTS = [
    RoadmapPoint("Initial gate", 14.0, "Basic graph/context gates started", 1, 14.0),
    RoadmapPoint("Compact proof", 40.0, "Compact proof/storage foundation", 2, 40.0),
    RoadmapPoint("Agent use", 50.5, "Agent JSON/context trust surfaces", 3, 50.5),
    RoadmapPoint("Stage 0 + planning", 61.5, "Text evidence + Buildroot planning", 4, 61.5),
    RoadmapPoint("Vector + nuance", 64.5, "Candidate recall, still non-proof", 5, 64.5),
    RoadmapPoint("Routing + chaos", 72.5, "MVP2 utility floor before scope expansion", 6, 72.5),
    RoadmapPoint("Benchmark v0", 74.0, "Retrieval diagnostics + claimability scoring", 7, 76.0),
    RoadmapPoint("MVP3/MVP4 scope rebased", 68.0, "Denominator expanded to validation + micro-flow future", 8, 86.0),
    RoadmapPoint("External-agent ready", 69.5, "Codex wrapper ready, no patch-quality score yet", 9, 88.0),
]

READINESS_BREAKDOWN = {
    "Context/retrieval": {"score": 30.0, "max": 35.0, "color": BLUE},
    "Trust/proof boundaries": {"score": 22.0, "max": 25.0, "color": GREEN},
    "Efficiency/ops": {"score": 10.0, "max": 15.0, "color": YELLOW},
    "Benchmark/outcome": {"score": 4.0, "max": 15.0, "color": PURPLE},
    "MVP3/MVP4 micro-flow readiness": {"score": 3.5, "max": 10.0, "color": DARK_BAR},
}


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def load_json(path: Path, label: str) -> dict[str, Any]:
    if not path.exists():
        raise SystemExit(f"required {label} missing: {rel(path)}")
    return json.loads(path.read_text(encoding="utf-8"))


def require_track(data: dict[str, Any], track: str) -> dict[str, Any]:
    summaries = data.get("track_summaries", {})
    if track not in summaries:
        raise SystemExit(f"benchmark track missing from full run report: {track}")
    return summaries[track]


def pct(value: float) -> float:
    return round(value * 100.0, 1)


def style_dark_axis(ax: plt.Axes) -> None:
    ax.set_facecolor(BG)
    for spine in ax.spines.values():
        spine.set_visible(False)
    ax.tick_params(colors=MUTED, labelsize=8)
    ax.yaxis.grid(True, color=GRID, linewidth=0.8, alpha=0.55)
    ax.xaxis.grid(False)


def save_readiness_chart() -> None:
    labels = [point.milestone for point in ROADMAP_POINTS]
    display_labels = [
        "Initial gate",
        "Compact proof",
        "Agent use",
        "Stage 0\n+ planning",
        "Vector\n+ nuance",
        "Routing\n+ chaos",
        "Benchmark\nv0",
        "MVP3/MVP4\nscope rebase",
        "External-agent\nready",
    ]
    scores = np.array([point.score for point in ROADMAP_POINTS], dtype=float)
    x = np.arange(len(labels))
    scope = np.array([point.scope_denominator for point in ROADMAP_POINTS], dtype=float)
    surfaces = np.array([point.verified_surfaces for point in ROADMAP_POINTS], dtype=float)
    surfaces_scaled = surfaces / max(surfaces) * 82.0

    fig, ax = plt.subplots(figsize=(12, 6.6), dpi=150)
    fig.patch.set_facecolor(BG)
    style_dark_axis(ax)

    ax.fill_between(x, scores, 0, color=WHITE, alpha=0.16)
    ax.plot(x, scores, color=WHITE, linewidth=2.6, marker="o", markersize=5.0, zorder=4)
    ax.step(x, surfaces_scaled, where="mid", color=GREY, linewidth=1.2, alpha=0.78, label="Verified gates / surfaces")
    ax.plot(x, scope, color="#7aa2d8", linewidth=1.1, linestyle=(0, (1, 2)), alpha=0.9, label="Scope expansion / denominator")

    for idx, score in enumerate(scores):
        if idx in {0, 1, 2, 3, 5, 6, 7, 8}:
            ax.text(idx, score + 3.2, f"{score:.1f}", color=TEXT, fontsize=8, ha="center", weight="bold")

    latest = scores[-1]
    ax.text(
        x[-1] + 0.05,
        latest + 4.5,
        f"{latest:.1f} / 100 current roadmap readiness",
        color=WHITE,
        fontsize=9,
        weight="bold",
    )
    ax.text(5, 77.5, "MVP2 utility floor", color=MUTED, fontsize=8, ha="center")
    ax.text(7, 60.0, "scope rebase", color=YELLOW, fontsize=8, ha="center")

    ax.set_ylim(0, 100)
    ax.set_xlim(-0.35, len(labels) - 0.15)
    ax.set_yticks([0, 25, 50, 75, 100])
    ax.set_xticks(x)
    ax.set_xticklabels(display_labels, rotation=0, ha="center", color=MUTED, fontsize=7)
    ax.set_title("Roadmap To MVP4 Agent Utility Readiness", color=TEXT, fontsize=17, loc="left", pad=18, weight="bold")
    ax.text(
        0.0,
        1.02,
        "Internal readiness score; real patch outcome benchmark is still near zero until focused SWE-bench Lite agent runs complete.",
        transform=ax.transAxes,
        color=MUTED,
        fontsize=9,
        ha="left",
    )

    # Latest contribution strip.
    strip_x = 0.66
    strip_y = 0.79
    strip_w = 0.28
    strip_h = 0.04
    cursor = strip_x
    for item in READINESS_BREAKDOWN.values():
        width = strip_w * (item["score"] / 100.0)
        fig.patches.append(
            plt.Rectangle(
                (cursor, strip_y),
                width,
                strip_h,
                transform=fig.transFigure,
                color=item["color"],
                linewidth=0,
                zorder=6,
            )
        )
        cursor += width
    fig.text(0.66, 0.84, "Current contribution", color=MUTED, fontsize=8, weight="bold")
    fig.text(
        0.66,
        0.745,
        "30 context | 22 trust | 10 ops | 4 outcome | 3.5 micro-flow",
        color=MUTED,
        fontsize=7.5,
    )

    ax.legend(loc="lower right", fontsize=7, frameon=False, labelcolor=MUTED)
    fig.text(
        0.05,
        0.035,
        "Internal readiness score, not official SWE-bench, RepoBench, CrossCodeEval, CGC, or rg comparison.",
        color=MUTED,
        fontsize=8,
    )
    fig.tight_layout(rect=[0.03, 0.07, 0.98, 0.93])
    fig.savefig(READINESS_CHART, facecolor=BG)
    plt.close(fig)


def save_retrieval_chart(full_run: dict[str, Any]) -> list[dict[str, Any]]:
    tracks = [
        ("Internal 20-task", "internal_gold"),
        ("RepoBench subset", "repobench_configured_subset"),
        ("CrossCodeEval subset", "crosscodeeval_configured_subset"),
    ]
    modes = ["rg_only", "codegraph_exact_text", "codegraph_full"]
    mode_labels = ["rg_only", "cg_exact", "cg_full"]
    colors = [GREY, BLUE, GREEN]
    metric_rows: list[dict[str, Any]] = []

    recall = []
    mrr = []
    for _, track_id in tracks:
        track = require_track(full_run, track_id)
        recall.append([pct(track[mode]["gold_file_recall_at_5"]) for mode in modes])
        mrr.append([pct(track[mode]["mrr"]) for mode in modes])
        for mode in modes:
            metric_rows.append(
                {
                    "track": track_id,
                    "mode": mode,
                    "tasks": track[mode]["tasks"],
                    "recall_at_5": track[mode]["gold_file_recall_at_5"],
                    "mrr": track[mode]["mrr"],
                    "claimability_violations": track[mode]["claimability_violations"],
                    "unsupported_claim_violations": track[mode]["unsupported_claim_violations"],
                }
            )

    fig, axes = plt.subplots(1, 2, figsize=(12, 4.8), dpi=150)
    fig.patch.set_facecolor(BG)
    width = 0.22
    x = np.arange(len(tracks))
    for ax, values, title in [
        (axes[0], np.array(recall), "Recall@5"),
        (axes[1], np.array(mrr), "MRR"),
    ]:
        style_dark_axis(ax)
        for idx, (label, color) in enumerate(zip(mode_labels, colors)):
            bars = ax.bar(x + (idx - 1) * width, values[:, idx], width, label=label, color=color)
            for bar in bars:
                ax.text(
                    bar.get_x() + bar.get_width() / 2,
                    bar.get_height() + 1.2,
                    f"{bar.get_height():.1f}",
                    color=TEXT,
                    fontsize=7,
                    ha="center",
                )
        ax.set_title(title, color=TEXT, fontsize=12, loc="left")
        ax.set_ylim(0, max(70, float(values.max()) + 12))
        ax.set_xticks(x)
        ax.set_xticklabels([item[0] for item in tracks], color=MUTED, fontsize=7)
        ax.set_ylabel("Percent-equivalent", color=MUTED, fontsize=8)
        ax.legend(loc="upper left", fontsize=7, frameon=False, labelcolor=MUTED)

    fig.suptitle("Retrieval Quality By Benchmark Track", color=TEXT, fontsize=16, weight="bold", x=0.05, ha="left")
    fig.text(
        0.05,
        0.035,
        "Local diagnostic product-ablation subsets. rg_only here is the bounded literal provider, not a strong human-style rg workflow.",
        color=MUTED,
        fontsize=8,
    )
    fig.tight_layout(rect=[0.03, 0.08, 0.98, 0.90])
    fig.savefig(RETRIEVAL_CHART, facecolor=BG)
    plt.close(fig)
    return metric_rows


def save_evidence_chart(full_run: dict[str, Any], manual: dict[str, Any]) -> dict[str, Any]:
    summary = manual["summary"]
    total = int(summary["labeled_samples"])
    true_positive = int(sum(int(row.get("true_positive", 0)) for row in summary["relation_precision"]))
    path_eval = manual["path_evidence_target_evaluation"]
    path_total = int(path_eval["labeled_samples"])
    path_precision = float(path_eval["precision"])
    claimability = int(full_run.get("claimability_violations_total", 0))
    unsupported = int(full_run.get("unsupported_claim_violations_total", 0))

    fig, ax = plt.subplots(figsize=(8.6, 4.8), dpi=150)
    fig.patch.set_facecolor(BG)
    style_dark_axis(ax)

    labels = ["claimability\nviolations", "unsupported-claim\nviolations", "sampled relation\nprecision", "PathEvidence\nsample"]
    values = [0, 0, true_positive / total * 100.0, path_precision * 100.0]
    display = ["0", "0", f"{true_positive}/{total}", f"{path_total}/{path_total}"]
    colors = [GREEN if claimability == 0 else RED, GREEN if unsupported == 0 else RED, BLUE, GREEN]

    x = np.arange(len(labels))
    bars = ax.bar(x, [2 if value == 0 else value for value in values], color=colors, width=0.62)
    for idx, bar in enumerate(bars):
        label_y = 5 if values[idx] == 0 else min(98, bar.get_height() + 3)
        ax.text(bar.get_x() + bar.get_width() / 2, label_y, display[idx], color=TEXT, fontsize=10, ha="center", weight="bold")
    ax.set_ylim(0, 110)
    ax.set_xticks(x)
    ax.set_xticklabels(labels, color=MUTED, fontsize=8)
    ax.set_yticks([0, 25, 50, 75, 100])
    ax.set_ylabel("Percent or zero-count gate", color=MUTED, fontsize=8)
    ax.set_title("Evidence Safety / Claim Boundary Health", color=TEXT, fontsize=15, loc="left", pad=12, weight="bold")
    fig.text(0.055, 0.035, "Sampled precision only; recall unknown. Text/vector/candidate evidence is not graph proof.", color=MUTED, fontsize=8)
    fig.tight_layout(rect=[0.03, 0.08, 0.98, 0.93])
    fig.savefig(EVIDENCE_CHART, facecolor=BG)
    plt.close(fig)

    return {
        "claimability_violations": claimability,
        "unsupported_claim_violations": unsupported,
        "manual_sampled_precision": {"true_positive": true_positive, "labeled_samples": total},
        "path_evidence_sample": {"correct": path_total, "labeled_samples": path_total},
    }


def save_swebench_chart(full_run: dict[str, Any], setup: dict[str, Any]) -> dict[str, Any]:
    swe = full_run["swe_bench_lite_gold_validation"]
    patch_status = full_run["patch_quality_status"]
    external_status = setup["tracks"]["external_agent_command"]["status"]
    steps = [
        ("checkout\npinned", True, "pinned\ncheckout"),
        ("linux\nharness", setup["tracks"]["swe_bench_lite"]["linux_container_harness"] == "ready", "linux\nready"),
        ("gold\n1/1", swe["status"] == "passed" and swe["resolved_instances"] == 1, "gold\npassed"),
        ("Codex wrapper\nready", True, "wrapper\nready"),
        ("patch-quality\npending", False, "agent cmd\nneeded"),
        ("official score\nnot claimed", False, "no public\nscore"),
    ]

    fig, ax = plt.subplots(figsize=(9.4, 4.6), dpi=150)
    fig.patch.set_facecolor(BG)
    style_dark_axis(ax)
    ax.yaxis.grid(False)
    ax.set_yticks([])
    ax.set_ylim(-0.6, 0.8)
    ax.set_xlim(-0.4, len(steps) - 0.6)

    for idx in range(len(steps) - 1):
        color = GREEN if steps[idx][1] and steps[idx + 1][1] else DARK_BAR
        ax.plot([idx, idx + 1], [0, 0], color=color, linewidth=3, alpha=0.9, zorder=1)
    for idx, (label, passed, note) in enumerate(steps):
        color = GREEN if passed else YELLOW if "pending" in label else DARK_BAR
        ax.scatter([idx], [0], s=260, color=color, edgecolor=WHITE, linewidth=1.0, zorder=3)
        ax.text(idx, 0.18, label, color=TEXT, fontsize=8, ha="center", va="bottom", weight="bold")
        ax.text(idx, -0.22, note, color=MUTED, fontsize=7, ha="center", va="top")

    ax.set_xticks([])
    ax.set_title("SWE-bench Readiness Ladder", color=TEXT, fontsize=15, loc="left", pad=12, weight="bold")
    fig.text(
        0.055,
        0.04,
        "Gold-validation smoke is harness readiness only. Patch quality waits on CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND; no official score claimed.",
        color=MUTED,
        fontsize=8,
    )
    fig.tight_layout(rect=[0.03, 0.10, 0.98, 0.92])
    fig.savefig(SWEBENCH_CHART, facecolor=BG)
    plt.close(fig)

    return {
        "gold_validation": swe,
        "patch_quality_status": patch_status,
        "external_agent_command_status": external_status,
        "steps": [
            {"label": label.replace("\n", " "), "ready": ready, "note": note}
            for label, ready, note in steps
        ],
    }


def validate_readme_images() -> None:
    if not README.exists():
        return
    text = README.read_text(encoding="utf-8")
    for path in [READINESS_CHART, RETRIEVAL_CHART, EVIDENCE_CHART, SWEBENCH_CHART]:
        if rel(path) not in text:
            raise SystemExit(f"README does not reference generated chart: {rel(path)}")
        if not path.exists() or path.stat().st_size == 0:
            raise SystemExit(f"generated chart missing or empty: {rel(path)}")


def write_manifest(
    full_run: dict[str, Any],
    retrieval_rows: list[dict[str, Any]],
    evidence: dict[str, Any],
    swebench: dict[str, Any],
) -> None:
    latest = ROADMAP_POINTS[-1]
    payload = {
        "schema_version": "readme_agent_readiness_manifest_v1",
        "generated_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
        "claim_boundary": "Internal roadmap-readiness and local diagnostic benchmark visuals only. Not an official SWE-bench, RepoBench, CrossCodeEval, CGC, or rg comparison.",
        "chart_paths": {
            "mvp4_readiness_over_time": rel(READINESS_CHART),
            "retrieval_quality_by_track": rel(RETRIEVAL_CHART),
            "evidence_safety": rel(EVIDENCE_CHART),
            "swebench_readiness_ladder": rel(SWEBENCH_CHART),
        },
        "roadmap_readiness": {
            "latest_score": latest.score,
            "latest_label": latest.milestone,
            "scoring_correction": "72.5 is the MVP2 verified agent-utility floor; 69.5 is current roadmap readiness after the MVP3/MVP4 denominator expansion.",
            "breakdown": READINESS_BREAKDOWN,
            "milestones": [point.__dict__ for point in ROADMAP_POINTS],
            "real_patch_outcome_note": "Real patch outcome still counts near zero until SWE-bench Lite external-agent runs complete.",
        },
        "retrieval_quality": {
            "run_id": full_run["run_id"],
            "claim_boundary": full_run["claim_boundary"],
            "rows": retrieval_rows,
        },
        "evidence_safety": evidence,
        "swebench_readiness": swebench,
        "source_files_used": [
            rel(BENCHMARK_FULL_RUN_JSON),
            rel(MANUAL_PRECISION_JSON),
            rel(SETUP_JSON),
            rel(PINNED_SOURCES_JSON),
        ],
        "caveats": [
            "Internal readiness score is not an official benchmark result.",
            "RepoBench and CrossCodeEval charts use configured local diagnostic subsets.",
            "The rg_only provider is bounded literal search, not the future strong human-style rg baseline.",
            "Sampled precision is not recall.",
            "SWE-bench gold validation is harness readiness, not agent quality.",
        ],
    }
    MANIFEST.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    ASSET_DIR.mkdir(parents=True, exist_ok=True)
    full_run = load_json(BENCHMARK_FULL_RUN_JSON, "benchmark full run scores")
    manual = load_json(MANUAL_PRECISION_JSON, "manual relation precision")
    setup = load_json(SETUP_JSON, "benchmark setup final unblock")
    load_json(PINNED_SOURCES_JSON, "pinned upstream sources")

    save_readiness_chart()
    retrieval_rows = save_retrieval_chart(full_run)
    evidence = save_evidence_chart(full_run, manual)
    swebench = save_swebench_chart(full_run, setup)
    write_manifest(full_run, retrieval_rows, evidence, swebench)
    validate_readme_images()

    for path in [READINESS_CHART, RETRIEVAL_CHART, EVIDENCE_CHART, SWEBENCH_CHART, MANIFEST]:
        if not path.exists() or path.stat().st_size == 0:
            raise SystemExit(f"generated artifact missing or empty: {rel(path)}")
        print(f"wrote {rel(path)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
