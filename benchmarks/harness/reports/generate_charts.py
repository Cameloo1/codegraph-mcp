from __future__ import annotations

import argparse
import json
from pathlib import Path


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("summary_json")
    parser.add_argument("--output-dir", default="benchmarks/results/summaries/charts")
    args = parser.parse_args(argv)
    generate_charts(Path(args.summary_json), Path(args.output_dir))
    return 0


def generate_charts(summary_json: Path, output_dir: Path) -> list[Path]:
    output_dir.mkdir(parents=True, exist_ok=True)
    summary = json.loads(summary_json.read_text(encoding="utf-8"))
    modes = summary.get("modes", {})
    charts = [
        _bar_chart(output_dir / "gold_file_recall_at_5.svg", "Gold File Recall@5", modes, "gold_file_recall_at_5", percent=True),
        _bar_chart(output_dir / "mrr.svg", "MRR", modes, "mrr", percent=True),
        _bar_chart(output_dir / "context_bytes.svg", "Context Bytes", modes, "context_bytes", percent=False),
        _bar_chart(output_dir / "tool_calls.svg", "Tool Calls", modes, "tool_calls", percent=False),
        _bar_chart(output_dir / "claimability_violations.svg", "Claimability Violations", modes, "claimability_violations", percent=False),
    ]
    return charts


def _bar_chart(path: Path, title: str, modes: dict, key: str, percent: bool) -> Path:
    width = 760
    row_h = 34
    height = 90 + max(1, len(modes)) * row_h
    values = [(mode, metrics.get(key) or 0) for mode, metrics in modes.items()]
    max_value = max([float(value) for _, value in values] + [1.0])
    rows = []
    for idx, (mode, value) in enumerate(values):
        y = 60 + idx * row_h
        bar_w = int((float(value) / max_value) * 460) if max_value else 0
        label = f"{float(value) * 100:.1f}%" if percent else f"{float(value):.1f}"
        rows.append(f'<text x="20" y="{y + 18}" font-size="14">{_esc(mode)}</text>')
        rows.append(f'<rect x="210" y="{y}" width="{bar_w}" height="22" fill="#2f7ebc"/>')
        rows.append(f'<text x="{220 + bar_w}" y="{y + 17}" font-size="13">{label}</text>')
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">\n'
        '<rect width="100%" height="100%" fill="white"/>\n'
        f'<text x="20" y="32" font-size="22" font-family="Arial">{_esc(title)}</text>\n'
        + "\n".join(rows)
        + "\n</svg>\n"
    )
    path.write_text(svg, encoding="utf-8")
    return path


def _esc(value: str) -> str:
    return value.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


if __name__ == "__main__":
    raise SystemExit(main())

