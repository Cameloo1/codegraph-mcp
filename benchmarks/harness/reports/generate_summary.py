from __future__ import annotations

import argparse
import json
from pathlib import Path


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("summary_json")
    parser.add_argument("--output", default=None)
    args = parser.parse_args(argv)
    path = Path(args.summary_json)
    summary = json.loads(path.read_text(encoding="utf-8"))
    output = Path(args.output) if args.output else path.with_suffix(".md")
    output.write_text(render_markdown(summary), encoding="utf-8")
    return 0


def render_markdown(summary: dict) -> str:
    lines = [
        "# Benchmark Summary",
        "",
        "Diagnostic/local result. Not a public benchmark claim.",
        "",
        "## Modes",
        "",
    ]
    for mode, metrics in summary.get("modes", {}).items():
        lines.append(f"- `{mode}`: {metrics.get('tasks', 0)} tasks, recall@5={metrics.get('gold_file_recall_at_5')}")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    raise SystemExit(main())

