#!/usr/bin/env python3
"""Generate the README MVP2-to-MVP4 roadmap chart.

The chart is a public README visual, so it uses only claim-bounded roadmap
state. It is not benchmark evidence, graph proof, release readiness, or a
patch-quality claim.
"""

from __future__ import annotations

import json
import os
import tempfile
import textwrap
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault(
    "MPLCONFIGDIR",
    str(Path(tempfile.gettempdir()) / "codegraph-mcp-matplotlib-cache"),
)

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np


ASSET_DIR = ROOT / "docs/assets/readme"
CHART_PATH = ASSET_DIR / "mvp2_to_mvp4_implementation_roadmap.png"

MVP3_9_HARNESS = ROOT / "reports/audit/mvp3_9_gate_harness_contract.json"
MVP3_9_INVENTORY = ROOT / "reports/audit/mvp3_9_gates_readiness_inventory.json"
PRODUCT_READINESS = ROOT / "reports/final/PRODUCT_READINESS.json"
ROADMAP_DOCS = [ROOT / "MVP_2.md", ROOT / "MVP_3.md", ROOT / "MVP_4.md"]

BG = "#07131d"
GRID = "#294057"
TEXT = "#e8eef8"
MUTED = "#b8c2d2"
SUBTLE = "#7f8ca3"
WHITE = "#f2f6fd"
AREA = "#667084"
BLUE = "#55b7e5"
GREEN = "#63d083"
YELLOW = "#f2cc4b"
PURPLE = "#bd7ce5"
DARK_BAR = "#3f4b60"


def hex_rgb(color: str) -> np.ndarray:
    color = color.lstrip("#")
    return np.array(
        [int(color[idx : idx + 2], 16) / 255.0 for idx in (0, 2, 4)],
        dtype=float,
    )


@dataclass(frozen=True)
class RoadmapPoint:
    label: str
    score: float
    phase: str
    status: str


ROADMAP_POINTS = [
    RoadmapPoint("Context\ntrust", 10.0, "MVP2", "complete"),
    RoadmapPoint("Routing\npacket", 22.0, "MVP2", "complete"),
    RoadmapPoint("Profile +\nRTDS", 34.0, "MVP2", "complete"),
    RoadmapPoint("Benchmark\nboundary", 42.0, "MVP2", "bounded"),
    RoadmapPoint("Reindex +\ndelta", 50.0, "MVP3", "complete"),
    RoadmapPoint("Validate +\ninterrupt", 58.0, "MVP3", "complete"),
    RoadmapPoint("Fixtures +\ngate harness", 66.0, "MVP3", "current"),
    RoadmapPoint("MVP3\nfinal gates", 72.0, "MVP3", "planned"),
    RoadmapPoint("Micro-flow\ncore", 82.0, "MVP4", "planned"),
    RoadmapPoint("Storage +\nrollout", 88.0, "MVP4", "planned"),
    RoadmapPoint("Routes +\ncontext", 95.0, "MVP4", "planned"),
    RoadmapPoint("MVP4\nfinal gate", 100.0, "MVP4", "planned"),
]

CURRENT_INDEX = 6
CURRENT_SCORE = ROADMAP_POINTS[CURRENT_INDEX].score

READINESS_BREAKDOWN = [
    ("MVP2 context/routing", 34.0, BLUE),
    ("MVP3 validation", 22.0, GREEN),
    ("Ops/outcome", 6.0, YELLOW),
    ("MVP4 prep", 4.0, PURPLE),
]


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise SystemExit(f"required source missing: {rel(path)}")
    return json.loads(path.read_text(encoding="utf-8-sig"))


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def validate_sources() -> None:
    for path in [*ROADMAP_DOCS, PRODUCT_READINESS, MVP3_9_HARNESS, MVP3_9_INVENTORY]:
        require(path.exists(), f"required source missing: {rel(path)}")

    harness = load_json(MVP3_9_HARNESS)
    inventory = load_json(MVP3_9_INVENTORY)
    product = load_json(PRODUCT_READINESS)

    require(harness.get("status") == "complete", "MVP3.9 gate harness is not complete")
    require(harness.get("ready_to_move_on") is True, "MVP3.9 gate harness is not ready to move on")
    require(harness.get("mvp4_not_started") is True, "MVP4 status is no longer future-only")
    require(inventory.get("mvp3_8_complete") is True, "MVP3.8 is not complete in current inventory")
    require(inventory.get("mvp3_9_allowed_to_start") is True, "MVP3.9 is not allowed to start")
    require(inventory.get("mvp4_not_started") is True, "MVP4 inventory boundary changed")
    require(product.get("public_claim") is False, "PRODUCT_READINESS public claim boundary changed")
    require(
        product.get("real_agent_patch_quality_claim") is False,
        "PRODUCT_READINESS patch-quality claim boundary changed",
    )


def style_axis(ax: plt.Axes) -> None:
    ax.set_facecolor((0, 0, 0, 0))
    for spine in ax.spines.values():
        spine.set_visible(False)
    ax.tick_params(colors=MUTED, labelsize=11, length=0, pad=11)
    ax.yaxis.grid(True, color=GRID, linewidth=0.9, alpha=0.7)
    ax.xaxis.grid(False)


def diffuse_background(width: int = 1600, height: int = 960) -> np.ndarray:
    base = hex_rgb(BG)
    yy, xx = np.mgrid[0:height, 0:width]
    img = np.ones((height, width, 3), dtype=float) * base

    glows = [
        (130, 70, 430, BLUE, 0.18),
        (1390, 95, 420, GREEN, 0.15),
        (990, 900, 480, YELLOW, 0.10),
        (1510, 820, 430, PURPLE, 0.13),
        (600, 555, 470, BLUE, 0.07),
    ]
    for cx, cy, radius, color, opacity in glows:
        dist2 = (xx - cx) ** 2 + (yy - cy) ** 2
        glow = np.exp(-dist2 / (2 * (radius * 0.58) ** 2))[:, :, None]
        target = hex_rgb(color)
        img = img * (1 - glow * opacity) + target * (glow * opacity)

    vertical = np.linspace(0.02, -0.015, height, dtype=float)[:, None, None]
    img = np.clip(img + vertical, 0.0, 1.0)
    return img


def add_background(fig: plt.Figure) -> None:
    bg_ax = fig.add_axes([0, 0, 1, 1], zorder=-10)
    bg_ax.imshow(diffuse_background(), extent=[0, 1, 0, 1], aspect="auto")
    bg_ax.set_axis_off()


def wrapped(text: str, width: int = 47) -> str:
    return textwrap.fill(text, width=width, break_long_words=False)


def fig_text(
    fig: plt.Figure,
    x: float,
    y: float,
    text: str,
    *,
    width: int = 47,
    size: float = 11.0,
    color: str = TEXT,
    weight: str = "normal",
    linespacing: float = 1.22,
) -> None:
    fig.text(
        x,
        y,
        wrapped(text, width),
        color=color,
        fontsize=size,
        ha="left",
        va="top",
        weight=weight,
        linespacing=linespacing,
    )


def add_phase_bands(fig: plt.Figure, ax: plt.Axes) -> None:
    bands = [
        (-0.45, 3.5, "MVP2: context and trust", BLUE),
        (3.5, 7.5, "MVP3: validate-edit loop", GREEN),
        (7.5, 11.45, "MVP4: micro-flow proof", PURPLE),
    ]
    xmin, xmax = ax.get_xlim()
    pos = ax.get_position()
    y0 = pos.y1 + 0.012
    height = 0.036
    for left, right, label, color in bands:
        left_frac = (left - xmin) / (xmax - xmin)
        right_frac = (right - xmin) / (xmax - xmin)
        x0 = pos.x0 + left_frac * pos.width
        width = (right_frac - left_frac) * pos.width
        fig.patches.append(
            plt.Rectangle(
                (x0, y0),
                width,
                height,
                transform=fig.transFigure,
                color=color,
                alpha=0.10,
                linewidth=0,
                zorder=2,
            )
        )
        fig.text(
            x0 + width / 2.0,
            y0 + height / 2.0,
            label,
            color=color,
            fontsize=9,
            ha="center",
            va="center",
            weight="bold",
            alpha=0.95,
        )


def add_readiness_bar(fig: plt.Figure) -> None:
    x0 = 0.705
    y0 = 0.250
    w = 0.235
    h = 0.041
    cursor = x0
    for _, value, color in READINESS_BREAKDOWN:
        width = w * (value / CURRENT_SCORE)
        fig.patches.append(
            plt.Rectangle(
                (cursor, y0),
                width,
                h,
                transform=fig.transFigure,
                color=color,
                linewidth=0,
                zorder=5,
            )
        )
        cursor += width
    if cursor < x0 + w:
        fig.patches.append(
            plt.Rectangle(
                (cursor,
                 y0),
                x0 + w - cursor,
                h,
                transform=fig.transFigure,
                color=DARK_BAR,
                linewidth=0,
                zorder=5,
            )
        )


def save_chart() -> None:
    validate_sources()
    ASSET_DIR.mkdir(parents=True, exist_ok=True)

    scores = np.array([point.score for point in ROADMAP_POINTS], dtype=float)
    x = np.arange(len(ROADMAP_POINTS))
    current_x = x[: CURRENT_INDEX + 1]
    future_x = x[CURRENT_INDEX:]

    fig = plt.figure(figsize=(16, 9.6), dpi=100)
    fig.patch.set_facecolor(BG)
    add_background(fig)
    ax = fig.add_axes([0.07, 0.18, 0.575, 0.60])
    style_axis(ax)

    ax.fill_between(current_x, scores[: CURRENT_INDEX + 1], 0, color=AREA, alpha=0.42)
    ax.fill_between(future_x, scores[CURRENT_INDEX:], 0, color=AREA, alpha=0.16)
    ax.plot(
        current_x,
        scores[: CURRENT_INDEX + 1],
        color=WHITE,
        linewidth=3.0,
        marker="o",
        markersize=6.5,
        zorder=6,
        label="implemented/current roadmap",
    )
    ax.plot(
        future_x,
        scores[CURRENT_INDEX:],
        color=WHITE,
        linewidth=2.1,
        marker="o",
        markersize=5.2,
        linestyle=(0, (5, 5)),
        alpha=0.72,
        zorder=5,
        label="planned path to MVP4",
    )
    verified_step = np.array([7, 15, 24, 32, 43, 54, 66], dtype=float)
    ax.step(
        current_x,
        verified_step,
        where="mid",
        color="#8fa1ba",
        linewidth=1.5,
        alpha=0.8,
        label="verified gates / surfaces",
    )

    ax.axvline(
        CURRENT_INDEX,
        ymin=0,
        ymax=CURRENT_SCORE / 100.0,
        color="#9aa9be",
        linewidth=1.7,
        alpha=0.75,
    )
    ax.text(
        CURRENT_INDEX + 0.10,
        CURRENT_SCORE + 4.0,
        "current",
        color=YELLOW,
        fontsize=10,
        ha="left",
        va="bottom",
        weight="bold",
    )

    for idx, score in enumerate(scores):
        color = TEXT if idx <= CURRENT_INDEX else MUTED
        dy = 3.0 if idx != len(scores) - 1 else -7.5
        va = "bottom" if dy > 0 else "top"
        ax.text(idx, score + dy, f"{score:.0f}", color=color, fontsize=9.5, ha="center", va=va, weight="bold")

    ax.set_xlim(-0.6, len(ROADMAP_POINTS) - 0.35)
    ax.set_ylim(0, 110)
    ax.set_yticks([0, 25, 50, 75, 100])
    ax.set_xticks(x)
    ax.set_xticklabels([point.label for point in ROADMAP_POINTS], color=MUTED, fontsize=9)
    add_phase_bands(fig, ax)
    ax.legend(loc="lower right", fontsize=8.5, frameon=False, labelcolor=MUTED)

    fig.text(
        0.055,
        0.90,
        "MVP2 To MVP4 Implementation Roadmap",
        color=TEXT,
        fontsize=25,
        weight="bold",
        ha="left",
        va="top",
    )

    right_x = 0.705
    fig.text(
        right_x,
        0.875,
        "MVP2 → MVP4 implementation\nroadmap",
        color=TEXT,
        fontsize=16,
        weight="bold",
        ha="left",
        va="top",
        linespacing=1.15,
    )
    fig.text(right_x, 0.785, "66 / 100", color=WHITE, fontsize=21, weight="bold", ha="left", va="top")
    fig.text(
        right_x + 0.118,
        0.782,
        "current internal roadmap readiness",
        color=MUTED,
        fontsize=10.4,
        ha="left",
        va="top",
    )

    fig_text(
        fig,
        right_x,
        0.665,
        "MVP2 purpose",
        width=47,
        size=13,
        weight="bold",
    )
    fig_text(
        fig,
        right_x,
        0.632,
        "Before editing, give agents lifecycle-safe context: exact seeds, candidate evidence, graph/source proof, unknowns, no-proof fallback, and compact routing packets.",
        width=47,
        size=9.6,
        color=MUTED,
    )

    fig_text(
        fig,
        right_x,
        0.548,
        "MVP3 purpose",
        width=47,
        size=13,
        weight="bold",
    )
    fig_text(
        fig,
        right_x,
        0.515,
        "After editing, reindex changed files, diff graph facts, classify stale evidence, and return blocking/warning/unknown packets before the agent continues.",
        width=47,
        size=9.6,
        color=MUTED,
    )

    fig_text(
        fig,
        right_x,
        0.431,
        "MVP4 purpose",
        width=47,
        size=13,
        weight="bold",
    )
    fig_text(
        fig,
        right_x,
        0.398,
        "Inside a function or file, emit source-spanned micro-flow packets plus route, bridge, and context surfaces without treating search or inference as proof.",
        width=49,
        size=9.15,
        color=MUTED,
    )

    fig.text(right_x, 0.327, "Readiness mix", color=TEXT, fontsize=15, weight="bold", ha="left", va="top")
    add_readiness_bar(fig)
    fig_text(
        fig,
        right_x,
        0.205,
        "Internal roadmap accounting only: shipped context/routing, validation gates, ops/outcome readiness, and early MVP4 prep.",
        width=47,
        size=9.7,
        color=MUTED,
    )
    fig.text(
        right_x,
        0.129,
        "34 MVP2 context | 22 MVP3 validation | 6 ops/outcome | 4 MVP4 prep",
        color=MUTED,
        fontsize=8.6,
        ha="left",
        va="top",
    )

    fig.text(
        0.055,
        0.035,
        "Internal readiness score only. Not official SWE-bench, RepoBench, CrossCodeEval, CGC, rg comparison, release, or patch-quality evidence.",
        color=MUTED,
        fontsize=9.5,
        ha="left",
        va="bottom",
    )

    fig.savefig(CHART_PATH, facecolor=BG)
    plt.close(fig)

    require(CHART_PATH.exists() and CHART_PATH.stat().st_size > 0, "chart was not generated")
    print(f"wrote {rel(CHART_PATH)}")


if __name__ == "__main__":
    save_chart()
