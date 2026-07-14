#!/usr/bin/env python3
"""Generate deterministic CodeGraph visual-system SVG assets.

The generator uses only the Python standard library. It produces public README
artifacts from a seed and does not inspect source code, local DBs, benchmark
outputs, or machine-specific paths.
"""

from __future__ import annotations

import argparse
import json
import math
import random
from dataclasses import dataclass
from html import escape
from pathlib import Path
from typing import Iterable


REPO_ROOT = Path(__file__).resolve().parents[2]
PRESETS_PATH = Path(__file__).with_name("proof_field_presets.json")
OUTPUT_ROOT = REPO_ROOT / "docs" / "assets" / "readme"

PALETTE = ["#1e8bff", "#1677f1", "#1bc8e5", "#17d9a1", "#8b78f6", "#d86bf2"]
DIM_PALETTE = ["#31465f", "#3d5065", "#465a72", "#35485d"]
VOID = "#030508"
INK = "#070a10"
SURFACE = "#0c1017"
DIVIDER = "#202630"
TEXT = "#f4f6f8"
MUTED = "#b6bdc8"
DIM = "#7e8795"
ROSE = "#f1788e"
AMBER = "#e7b85a"


@dataclass(frozen=True)
class Point:
    x: float
    y: float


def load_presets() -> dict[str, dict[str, object]]:
    return json.loads(PRESETS_PATH.read_text(encoding="utf-8"))


def fmt(value: float) -> str:
    return f"{value:.2f}".rstrip("0").rstrip(".")


def attrs(**values: object) -> str:
    rendered: list[str] = []
    for key, value in values.items():
        if value is None:
            continue
        rendered.append(f'{key.replace("_", "-")}="{escape(str(value), quote=True)}"')
    return " ".join(rendered)


def svg_text(x: float, y: float, value: str, class_name: str, anchor: str | None = None) -> str:
    return (
        f'<text {attrs(x=fmt(x), y=fmt(y), class_=class_name, text_anchor=anchor)}>'
        f"{escape(value)}</text>"
    ).replace('class-="', 'class="')


def cubic(start: Point, control_a: Point, control_b: Point, end: Point) -> str:
    return (
        f"M{fmt(start.x)},{fmt(start.y)} "
        f"C{fmt(control_a.x)},{fmt(control_a.y)} "
        f"{fmt(control_b.x)},{fmt(control_b.y)} "
        f"{fmt(end.x)},{fmt(end.y)}"
    )


def two_segment(
    start: Point,
    first_a: Point,
    first_b: Point,
    middle: Point,
    second_a: Point,
    second_b: Point,
    end: Point,
) -> str:
    return (
        f"M{fmt(start.x)},{fmt(start.y)} "
        f"C{fmt(first_a.x)},{fmt(first_a.y)} {fmt(first_b.x)},{fmt(first_b.y)} "
        f"{fmt(middle.x)},{fmt(middle.y)} "
        f"C{fmt(second_a.x)},{fmt(second_a.y)} {fmt(second_b.x)},{fmt(second_b.y)} "
        f"{fmt(end.x)},{fmt(end.y)}"
    )


def svg_header(width: int, height: int, title: str, description: str) -> list[str]:
    return [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">',
        f'<title id="title">{escape(title)}</title>',
        f'<desc id="desc">{escape(description)}</desc>',
        "<defs>",
        '<filter id="selected-glow" x="-30%" y="-30%" width="160%" height="160%">',
        '<feGaussianBlur stdDeviation="2.4" result="blur"/>',
        "<feMerge><feMergeNode in=\"blur\"/><feMergeNode in=\"SourceGraphic\"/></feMerge>",
        "</filter>",
        '<filter id="grain" x="0" y="0" width="100%" height="100%">',
        '<feTurbulence type="fractalNoise" baseFrequency="0.75" numOctaves="3" stitchTiles="stitch"/>',
        '<feColorMatrix type="saturate" values="0"/>',
        "</filter>",
        "<style>",
        ".display{font-family:Arial,Helvetica,sans-serif;fill:#f4f6f8;font-weight:600;letter-spacing:-0.045em}",
        ".body{font-family:Arial,Helvetica,sans-serif;fill:#b6bdc8}",
        ".mono{font-family:Consolas,Menlo,monospace;fill:#7e8795;letter-spacing:.06em}",
        ".guide{fill:none;stroke:#b6bdc8;stroke-width:1;stroke-dasharray:2 10;opacity:.12}",
        ".axis{fill:none;stroke:#b6bdc8;stroke-width:1;opacity:.055}",
        ".path{fill:none;stroke-linecap:round;stroke-linejoin:round}",
        ".label{font-family:Consolas,Menlo,monospace;fill:#697382;font-size:10px;letter-spacing:.03em}",
        ".small{font-size:12px}",
        ".tiny{font-size:10px}",
        "@media(prefers-reduced-motion:no-preference){.flow-a{stroke-dasharray:28 420;animation:flow 12s linear infinite}.flow-b{stroke-dasharray:18 360;animation:flow 16s linear infinite reverse}@keyframes flow{to{stroke-dashoffset:-448}}}",
        "</style>",
        "</defs>",
    ]


def add_background(lines: list[str], width: int, height: int, *, grid: bool = False) -> None:
    lines.append(f'<rect width="{width}" height="{height}" fill="{VOID}"/>')
    if grid:
        for x in range(0, width + 1, 64):
            lines.append(f'<line class="axis" x1="{x}" x2="{x}" y1="0" y2="{height}"/>')
        for y in range(0, height + 1, 64):
            lines.append(f'<line class="axis" x1="0" x2="{width}" y1="{y}" y2="{y}"/>')
    lines.append(
        f'<rect width="{width}" height="{height}" filter="url(#grain)" opacity=".025" style="mix-blend-mode:soft-light"/>'
    )


def add_guides(lines: list[str], point: Point, max_radius: float, rings: int = 5) -> None:
    for index in range(rings):
        radius = max_radius * (0.3 + index * 0.175)
        lines.append(
            f'<circle class="guide" cx="{fmt(point.x)}" cy="{fmt(point.y)}" r="{fmt(radius)}" opacity="{fmt(0.2 - index * 0.025)}"/>'
        )
    lines.append(
        f'<line class="axis" x1="{fmt(point.x - max_radius)}" x2="{fmt(point.x + max_radius)}" y1="{fmt(point.y)}" y2="{fmt(point.y)}"/>'
    )
    lines.append(
        f'<line class="axis" x1="{fmt(point.x)}" x2="{fmt(point.x)}" y1="{fmt(point.y - max_radius)}" y2="{fmt(point.y + max_radius)}"/>'
    )


def add_node(lines: list[str], point: Point, color: str, radius: float = 3.5) -> None:
    lines.append(
        f'<circle cx="{fmt(point.x)}" cy="{fmt(point.y)}" r="{fmt(radius * 4.5)}" class="guide" opacity=".32"/>'
    )
    lines.append(
        f'<circle cx="{fmt(point.x)}" cy="{fmt(point.y)}" r="{fmt(radius)}" fill="{color}" stroke="{VOID}" stroke-width="1"/>'
    )


def add_dot_cloud(
    lines: list[str],
    rng: random.Random,
    center: Point,
    spread_x: float,
    spread_y: float,
    count: int,
    colors: Iterable[str] = PALETTE,
) -> None:
    colors = tuple(colors)
    for index in range(count):
        angle = rng.random() * math.tau
        radius = math.sqrt(rng.random())
        x = center.x + math.cos(angle) * spread_x * radius
        y = center.y + math.sin(angle) * spread_y * radius
        size = 0.9 + rng.random() * 1.7
        color = colors[index % len(colors)]
        opacity = 0.28 + rng.random() * 0.5
        lines.append(
            f'<circle cx="{fmt(x)}" cy="{fmt(y)}" r="{fmt(size)}" fill="{color}" opacity="{fmt(opacity)}"/>'
        )


def generate_title(config: dict[str, object]) -> str:
    width = int(config["width"])
    height = int(config["height"])
    rng = random.Random(str(config["seed"]))
    verify = Point(width * float(config["attractor"][0]), height * float(config["attractor"][1]))
    packet = Point(width * float(config["packet"][0]), height * float(config["packet"][1]))
    path_count = int(config["paths"])

    lines = svg_header(
        width,
        height,
        "CodeGraph verified signal field",
        "A dark editorial CodeGraph banner. Candidate lanes converge at graph and source verification. Supported paths continue as source-spanned evidence.",
    )
    add_background(lines, width, height, grid=True)
    lines.append('<clipPath id="visual-clip"><rect x="560" y="0" width="1040" height="720"/></clipPath>')
    lines.append('<g clip-path="url(#visual-clip)">')
    add_guides(lines, verify, 260, 6)
    add_guides(lines, packet, 150, 4)

    for index in range(path_count):
        group = index % 4
        if group == 0:
            start = Point(545 + rng.random() * 650, -24)
        elif group == 1:
            start = Point(520 + rng.random() * 760, height + 24)
        elif group == 2:
            start = Point(540, 45 + rng.random() * 620)
        else:
            start = Point(width + 25, 35 + rng.random() * 650)

        verified = rng.random() > 0.25
        middle = Point(verify.x + (rng.random() - 0.5) * 17, verify.y + (rng.random() - 0.5) * 15)
        if verified:
            end = Point(width + 30, 70 + rng.random() * 580)
        else:
            end = Point(verify.x + 120 + rng.random() * 230, 85 + rng.random() * 560)

        first_a = Point(
            start.x + (verify.x - start.x) * (0.28 + rng.random() * 0.14),
            start.y + (verify.y - start.y) * (0.1 + rng.random() * 0.4),
        )
        first_b = Point(verify.x - 100 - rng.random() * 95, verify.y + (rng.random() - 0.5) * 100)
        second_a = Point(verify.x + 70 + rng.random() * 80, verify.y + (rng.random() - 0.5) * 90)
        second_b = Point(end.x - 180 - rng.random() * 160, end.y + (rng.random() - 0.5) * 150)
        color = PALETTE[index % len(PALETTE)] if verified else DIM_PALETTE[index % len(DIM_PALETTE)]
        opacity = 0.24 + rng.random() * (0.56 if verified else 0.24)
        stroke_width = 0.7 + rng.random() * (1.25 if verified else 0.7)
        dash = "" if verified else ' stroke-dasharray="4 8"'
        animation = " flow-a" if verified and index % 13 == 0 else ""
        lines.append(
            f'<path class="path{animation}" d="{two_segment(start, first_a, first_b, middle, second_a, second_b, end)}" '
            f'stroke="{color}" stroke-width="{fmt(stroke_width)}" opacity="{fmt(opacity)}"{dash}/>'
        )

    add_dot_cloud(lines, rng, Point(verify.x - 115, verify.y + 28), 120, 92, 78)
    add_node(lines, verify, "#17d9a1", 4)
    add_node(lines, packet, "#1bc8e5", 3.2)
    lines.append(svg_text(verify.x + 13, verify.y - 15, "graph/source verify", "label"))
    lines.append(svg_text(packet.x + 12, packet.y - 13, "compact packet", "label"))
    lines.append("</g>")

    lines.extend(
        [
            svg_text(82, 116, "codegraph-mcp", "mono small"),
            svg_text(82, 252, "A verified map", "display", None).replace(">A verified", ' font-size="78">A verified'),
            svg_text(82, 330, "for coding agents.", "display", None).replace(">for coding", ' font-size="78">for coding'),
            svg_text(82, 404, "Local, proof-grounded repository context with typed paths,", "body", None).replace(">Local", ' font-size="19">Local'),
            svg_text(82, 432, "source spans, exactness labels, and explicit unknowns.", "body", None).replace(">source", ' font-size="19">source'),
            '<line x1="82" x2="480" y1="517" y2="517" stroke="#202630"/>',
            svg_text(82, 556, "VECTORS SUGGEST", "mono tiny"),
            svg_text(219, 556, "GRAPH VERIFIES", "mono tiny"),
            svg_text(356, 556, "SOURCE SPANS SUPPORT", "mono tiny"),
            '<circle cx="82" cy="612" r="4" fill="#17d9a1"/>',
            svg_text(97, 616, "local only · read-mostly MCP · deterministic graph", "body", None).replace(">local", ' font-size="13">local'),
        ]
    )
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def panel_paths(
    lines: list[str],
    rng: random.Random,
    left: float,
    top: float,
    width: float,
    height: float,
    mode: str,
    count: int,
) -> None:
    attractor = Point(left + width * 0.52, top + height * 0.68)
    add_guides(lines, attractor, min(width, height) * 0.23, 4)
    for index in range(count):
        start = Point(left + width * (0.04 + rng.random() * 0.92), top - 18)
        reaches = mode == "verified" or (mode == "candidate" and rng.random() > 0.38) or (mode == "unknown" and rng.random() > 0.78)
        middle = (
            Point(attractor.x + (rng.random() - 0.5) * 10, attractor.y + (rng.random() - 0.5) * 12)
            if reaches
            else Point(left + width * (0.18 + rng.random() * 0.64), top + height * (0.36 + rng.random() * 0.28))
        )
        end = Point(left + width * (0.08 + rng.random() * 0.84), top + height + 18) if reaches else middle
        color = (
            PALETTE[index % 4]
            if mode == "verified"
            else ("#1e8bff" if mode == "candidate" and index % 3 == 0 else DIM_PALETTE[index % len(DIM_PALETTE)])
            if mode == "candidate"
            else ROSE
        )
        opacity = 0.3 + rng.random() * (0.52 if mode == "verified" else 0.3)
        dash = "" if mode == "verified" else (' stroke-dasharray="5 8"' if mode == "candidate" else ' stroke-dasharray="2 8"')
        if reaches:
            path = two_segment(
                start,
                Point(start.x + (rng.random() - 0.5) * 110, top + height * 0.24),
                Point(attractor.x + (rng.random() - 0.5) * 60, attractor.y - height * 0.16),
                middle,
                Point(attractor.x + (rng.random() - 0.5) * 60, attractor.y + height * 0.11),
                Point(end.x + (rng.random() - 0.5) * 95, top + height * 0.9),
                end,
            )
        else:
            path = cubic(
                start,
                Point(start.x + (rng.random() - 0.5) * 100, top + height * 0.23),
                Point(middle.x + (rng.random() - 0.5) * 80, middle.y - height * 0.14),
                middle,
            )
        lines.append(
            f'<path class="path" d="{path}" stroke="{color}" stroke-width="{fmt(0.7 + rng.random() * 1.05)}" opacity="{fmt(opacity)}"{dash}/>'
        )
    add_node(lines, attractor, "#17d9a1" if mode == "verified" else "#1e8bff" if mode == "candidate" else ROSE, 3.7)


def generate_taxonomy(config: dict[str, object]) -> str:
    width = int(config["width"])
    height = int(config["height"])
    rng = random.Random(str(config["seed"]))
    count = int(config["paths_per_panel"])
    lines = svg_header(
        width,
        height,
        "CodeGraph evidence taxonomy",
        "Three panels distinguish verified graph evidence, candidate evidence, and explicit unknowns using color and line pattern.",
    )
    add_background(lines, width, height)
    margin = 56
    gutter = 22
    panel_width = (width - margin * 2 - gutter * 2) / 3
    panel_top = 46
    panel_height = 570
    modes = [
        ("verified", "Verified", "graph/source supported", "solid paths · source spans"),
        ("candidate", "Candidate", "useful for inspection", "dashed paths · not proof"),
        ("unknown", "Unknown", "unsupported stays explicit", "open routes · no promotion"),
    ]

    for panel_index, (mode, title, subtitle, footer) in enumerate(modes):
        left = margin + panel_index * (panel_width + gutter)
        lines.append(
            f'<rect x="{fmt(left)}" y="{panel_top}" width="{fmt(panel_width)}" height="{panel_height}" fill="#06090e" stroke="#202630"/>'
        )
        panel_paths(lines, rng, left, panel_top, panel_width, panel_height - 130, mode, count)
        lines.append(
            f'<line x1="{fmt(left + 20)}" x2="{fmt(left + panel_width - 20)}" y1="{panel_top + panel_height - 118}" y2="{panel_top + panel_height - 118}" stroke="#202630"/>'
        )
        lines.append(svg_text(left + 20, panel_top + panel_height - 88, title.upper(), "mono tiny"))
        lines.append(svg_text(left + 20, panel_top + panel_height - 54, subtitle, "display").replace(f">{escape(subtitle)}", f' font-size="24">{escape(subtitle)}'))
        lines.append(svg_text(left + 20, panel_top + panel_height - 25, footer, "body").replace(f">{escape(footer)}", f' font-size="12">{escape(footer)}'))

    lines.append(svg_text(width / 2, 654, "candidate evidence never masquerades as graph proof", "mono tiny", "middle"))
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def loop_stage(lines: list[str], x: float, y: float, title: str, body: list[str], color: str) -> None:
    lines.append(f'<circle cx="{fmt(x)}" cy="{fmt(y)}" r="7" fill="{color}" stroke="{VOID}" stroke-width="2"/>')
    lines.append(f'<circle class="guide" cx="{fmt(x)}" cy="{fmt(y)}" r="23"/>')
    lines.append(svg_text(x, y + 63, title, "display", "middle").replace(f">{escape(title)}", f' font-size="25">{escape(title)}'))
    for index, value in enumerate(body):
        lines.append(svg_text(x, y + 92 + index * 22, value, "body", "middle").replace(f">{escape(value)}", f' font-size="13">{escape(value)}'))


def generate_agent_loop(config: dict[str, object]) -> str:
    width = int(config["width"])
    height = int(config["height"])
    lines = svg_header(
        width,
        height,
        "CodeGraph agent-use loop",
        "Normal developer tools and CodeGraph proof context operate together. The agent inspects, edits, validates changed files, and runs project tests.",
    )
    add_background(lines, width, height, grid=True)
    lines.append(svg_text(80, 86, "How to use CodeGraph", "display").replace(">How to use", ' font-size="52">How to use'))
    lines.append(svg_text(80, 124, "Use proof-grounded context at decision points. Keep normal tools in the loop.", "body").replace(">Use proof", ' font-size="17">Use proof'))

    stages = [
        (165, 320, "Status", ["lifecycle", "claimability"], "#1e8bff"),
        (425, 250, "Context", ["likely files", "proof labels"], "#1bc8e5"),
        (685, 320, "Inspect", ["symbols · paths", "source spans"], "#17d9a1"),
        (945, 250, "Edit", ["search · read", "change files"], "#17d9a1"),
        (1205, 320, "Validate", ["blockers", "warnings · unknowns"], "#8b78f6"),
        (1435, 250, "Tests", ["compiler", "project checks"], "#d86bf2"),
    ]

    path_points = [Point(x, y) for x, y, *_ in stages]
    for index in range(len(path_points) - 1):
        start = path_points[index]
        end = path_points[index + 1]
        color = stages[index][4]
        lines.append(
            f'<path class="path flow-a" d="{cubic(start, Point(start.x + 95, start.y), Point(end.x - 95, end.y), end)}" stroke="{color}" stroke-width="2" opacity=".75"/>'
        )

    loop_start = path_points[-1]
    loop_end = path_points[1]
    lines.append(
        f'<path class="path" d="M{fmt(loop_start.x)},{fmt(loop_start.y)} C{fmt(loop_start.x + 35)},{fmt(loop_start.y + 245)} {fmt(loop_end.x - 55)},{fmt(loop_end.y + 310)} {fmt(loop_end.x)},{fmt(loop_end.y + 26)}" stroke="#4d5b6c" stroke-width="1.3" stroke-dasharray="5 9" opacity=".58"/>'
    )
    lines.append(svg_text(892, 652, "repeat after meaningful edit batches", "mono tiny", "middle"))

    for stage in stages:
        loop_stage(lines, stage[0], stage[1], stage[2], stage[3], stage[4])

    lines.append('<line x1="80" x2="1520" y1="690" y2="690" stroke="#202630"/>')
    lines.append(svg_text(80, 725, "normal search, editing, and tests remain first-class", "mono tiny"))
    lines.append(svg_text(1520, 725, "graph/source verification is the proof boundary", "mono tiny", "end"))
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def generate_terminal(config: dict[str, object]) -> str:
    width = int(config["width"])
    height = int(config["height"])
    lines = svg_header(
        width,
        height,
        "CodeGraph agent-use command sequence",
        "An illustrative terminal sequence showing lifecycle status, compact context, changed-file validation, and project tests. It contains no machine-local path or benchmark result.",
    )
    add_background(lines, width, height)
    lines.append(f'<rect x="36" y="34" width="{width - 72}" height="{height - 68}" rx="7" fill="#080c12" stroke="#252d38"/>')
    lines.append(f'<rect x="36" y="34" width="{width - 72}" height="48" rx="7" fill="#101620"/>')
    lines.append('<circle cx="60" cy="58" r="5" fill="#f1788e"/><circle cx="80" cy="58" r="5" fill="#e7b85a"/><circle cx="100" cy="58" r="5" fill="#17d9a1"/>')
    lines.append(svg_text(124, 63, "illustrative local workflow", "mono tiny"))

    commands = [
        (118, "# 1. inspect lifecycle and claimability", DIM),
        (151, "$ codegraph-mcp agent-use status --repo /workspace --json", TEXT),
        (208, "# 2. request bounded, proof-labeled context", DIM),
        (241, '$ codegraph-mcp agent-use context-pack --repo /workspace \\', TEXT),
        (272, '    --task "Trace the change impact" --agent-json', TEXT),
        (329, "# 3. validate the changed-file batch", DIM),
        (362, '$ codegraph-mcp agent-use validate-edit --repo /workspace \\', TEXT),
        (393, "    --changed src/auth.ts --agent-json", TEXT),
        (450, "# 4. run the project checks before completion", DIM),
        (483, "$ cargo test --workspace", TEXT),
    ]
    for index, (y, value, color) in enumerate(commands):
        class_name = "mono"
        lines.append(
            f'<text x="76" y="{y}" class="{class_name}" font-size="18" fill="{color}" opacity="{0.72 if value.startswith("#") else 1}">{escape(value)}</text>'
        )
        if not value.startswith("#"):
            lines.append(
                f'<rect x="70" y="{y - 23}" width="6" height="28" fill="{PALETTE[index % len(PALETTE)]}" opacity=".58"/>'
            )

    lines.append(f'<rect x="70" y="540" width="{width - 140}" height="1" fill="#202630"/>')
    lines.append(svg_text(76, 583, "CodeGraph adds context and validation beside normal developer tools.", "body").replace(">CodeGraph", ' font-size="17">CodeGraph'))
    lines.append(svg_text(76, 616, "Candidate evidence remains labeled until graph/source verification succeeds.", "body").replace(">Candidate", ' font-size="17">Candidate'))
    lines.append(svg_text(76, 667, "local only", "mono tiny"))
    lines.append(svg_text(width - 76, 667, "no source edits performed by the MCP surface", "mono tiny", "end"))
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def write_asset(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8", newline="\n")
    print(f"wrote {path.relative_to(REPO_ROOT)}")


def generate_all() -> None:
    presets = load_presets()
    write_asset(OUTPUT_ROOT / "title-pic.svg", generate_title(presets["title"]))
    write_asset(OUTPUT_ROOT / "proof_taxonomy.svg", generate_taxonomy(presets["taxonomy"]))
    write_asset(OUTPUT_ROOT / "agent_use_loop.svg", generate_agent_loop(presets["agent_loop"]))
    write_asset(OUTPUT_ROOT / "codegraph_terminal.svg", generate_terminal(presets["terminal"]))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--all",
        action="store_true",
        help="generate every promoted CodeGraph visual-system asset (default)",
    )
    return parser.parse_args()


def main() -> int:
    parse_args()
    generate_all()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
