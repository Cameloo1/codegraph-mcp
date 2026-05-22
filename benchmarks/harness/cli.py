from __future__ import annotations

import argparse

from benchmarks.harness.runners.run_retrieval_eval import main as retrieval_main
from benchmarks.harness.runners.run_smoke_suite import main as smoke_main


def main() -> int:
    parser = argparse.ArgumentParser(prog="python -m benchmarks.harness.cli")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("smoke")
    sub.add_parser("retrieval")
    args, rest = parser.parse_known_args()
    if args.command == "smoke":
        return smoke_main(rest)
    if args.command == "retrieval":
        return retrieval_main(rest)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())

