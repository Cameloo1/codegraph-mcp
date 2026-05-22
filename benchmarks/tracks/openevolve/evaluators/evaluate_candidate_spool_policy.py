"""Evaluator for the CodeGraph candidate-spool OpenEvolve policy lab."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import time
from pathlib import Path
from typing import Any, Dict, Iterable, List, Mapping, Sequence, Tuple


ROOT = Path(__file__).resolve().parents[1]
FIXTURE_DIR = ROOT / "fixtures" / "candidate_spool_policy"
INVENTORY_PATH = FIXTURE_DIR / "candidate_inventory_sample.jsonl"
GOLD_PATH = FIXTURE_DIR / "gold_queries.json"


REQUIRED_FUNCTIONS = (
    "score_candidate",
    "packet_key",
    "should_keep_candidate",
    "rank_packet",
    "select_packets",
)


def _load_program(program_path: str):
    spec = importlib.util.spec_from_file_location("candidate_spool_policy_under_test", program_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to import program: {program_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules["candidate_spool_policy_under_test"] = module
    spec.loader.exec_module(module)
    missing = [name for name in REQUIRED_FUNCTIONS if not hasattr(module, name)]
    if missing:
        raise RuntimeError(f"missing required function(s): {', '.join(missing)}")
    return module


def _load_jsonl(path: Path) -> List[Dict[str, Any]]:
    rows: List[Dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise RuntimeError(f"{path}:{line_no}: invalid JSONL row: {exc}") from exc
    return rows


def _canonical(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def _packet_bytes(packets_by_query: Mapping[str, Sequence[Mapping[str, Any]]]) -> int:
    return len(_canonical(packets_by_query).encode("utf-8"))


def _packet_claim_violations(packets: Iterable[Mapping[str, Any]]) -> Tuple[int, int]:
    claimability = 0
    unsupported = 0
    proof_like = {"graph_relation_proof", "mutation_proof", "flow_proof", "graph_proof"}
    for packet in packets:
        evidence_kind = str(packet.get("evidence_kind", "candidate_evidence"))
        if packet.get("graph_proof") or packet.get("claimable_graph"):
            claimability += 1
        if evidence_kind in proof_like and not packet.get("verified_by_graph_source"):
            unsupported += 1
        for member in packet.get("members", []) or []:
            claimability_map = member.get("claimability") or {}
            if (
                member.get("graph_proof")
                or member.get("claimable_graph")
                or claimability_map.get("graph_proof")
                or claimability_map.get("claimable_graph")
            ):
                claimability += 1
    return claimability, unsupported


def _packet_paths(packets: Sequence[Mapping[str, Any]]) -> List[str]:
    paths: List[str] = []
    for packet in packets:
        path = packet.get("path")
        if isinstance(path, str) and path and path not in paths:
            paths.append(path)
    return paths


def _packet_symbols(packets: Sequence[Mapping[str, Any]]) -> List[str]:
    symbols: List[str] = []
    for packet in packets:
        for symbol in packet.get("symbols", []) or []:
            symbol_s = str(symbol)
            if symbol_s not in symbols:
                symbols.append(symbol_s)
    return symbols


def _recall_at_k(actual: Sequence[str], expected: Sequence[str], k: int) -> float:
    if not expected:
        return 1.0
    top = set(actual[:k])
    return len(top & set(expected)) / len(set(expected))


def _mrr(actual: Sequence[str], expected: Sequence[str]) -> float:
    expected_set = set(expected)
    if not expected_set:
        return 1.0
    for index, value in enumerate(actual, start=1):
        if value in expected_set:
            return 1.0 / index
    return 0.0


def _diversity(candidates: Sequence[Mapping[str, Any]], packets_by_query: Mapping[str, Sequence[Mapping[str, Any]]]) -> float:
    paths = set()
    kinds = set()
    sources = set()
    for packets in packets_by_query.values():
        for packet in packets:
            if packet.get("path"):
                paths.add(packet["path"])
            if packet.get("candidate_kind"):
                kinds.add(packet["candidate_kind"])
            if packet.get("source_kind"):
                sources.add(packet["source_kind"])
    denominator = max(1, min(18, len(candidates)))
    return min(1.0, (len(paths) + len(kinds) + len(sources)) / denominator)


def _run_once(module: Any, candidates: Sequence[Mapping[str, Any]], queries: Sequence[Mapping[str, Any]]) -> Dict[str, Any]:
    packets_by_query: Dict[str, List[Mapping[str, Any]]] = {}
    for query in queries:
        query_id = str(query["query_id"])
        budget = query.get("budget") or {}
        selected = module.select_packets(candidates, query, budget)
        if not isinstance(selected, list):
            raise RuntimeError(f"select_packets returned {type(selected).__name__}, expected list")
        packets_by_query[query_id] = selected
    return packets_by_query


def evaluate(program_path: str) -> Dict[str, Any]:
    try:
        module = _load_program(program_path)
        candidates = _load_jsonl(INVENTORY_PATH)
        queries = json.loads(GOLD_PATH.read_text(encoding="utf-8"))["queries"]

        started = time.perf_counter()
        first = _run_once(module, candidates, queries)
        second = _run_once(module, candidates, queries)
        elapsed_ms = (time.perf_counter() - started) * 1000.0

        if _canonical(first) != _canonical(second):
            return {
                "combined_score": 0.0,
                "score": 0.0,
                "error": "nondeterministic output",
                "claimability_violations": 0,
                "unsupported_claim_violations": 0,
            }

        file_recalls: List[float] = []
        symbol_recalls: List[float] = []
        mrrs: List[float] = []
        missing_required: List[str] = []
        claimability_violations = 0
        unsupported_claim_violations = 0

        for query in queries:
            query_id = str(query["query_id"])
            packets = first[query_id]
            paths = _packet_paths(packets)
            symbols = _packet_symbols(packets)
            packet_claims, packet_unsupported = _packet_claim_violations(packets)
            claimability_violations += packet_claims
            unsupported_claim_violations += packet_unsupported

            gold_files = query.get("gold_files", []) or []
            gold_symbols = query.get("gold_symbols", []) or []
            file_recall = _recall_at_k(paths, gold_files, 5)
            symbol_recall = _recall_at_k(symbols, gold_symbols, 5)
            file_recalls.append(file_recall)
            symbol_recalls.append(symbol_recall)
            mrrs.append(_mrr(paths, gold_files))

            if query.get("required", True) and file_recall < 1.0:
                missing_required.append(f"{query_id}:files")
            if query.get("required_symbols", False) and symbol_recall < 1.0:
                missing_required.append(f"{query_id}:symbols")

        bytes_used = _packet_bytes(first)
        packet_count = sum(len(packets) for packets in first.values())
        max_bytes = max(int((query.get("budget") or {}).get("max_packet_bytes", 7000)) for query in queries)
        byte_score = max(0.0, min(1.0, 1.0 - (bytes_used / max(1, max_bytes * len(queries) * 1.25))))
        latency_score = 1.0 / (1.0 + (elapsed_ms / 50.0))
        recall_at_5 = sum(file_recalls) / max(1, len(file_recalls))
        symbol_recall_at_5 = sum(symbol_recalls) / max(1, len(symbol_recalls))
        mrr = sum(mrrs) / max(1, len(mrrs))
        diversity = _diversity(candidates, first)

        if missing_required or claimability_violations or unsupported_claim_violations:
            combined = 0.0
        else:
            combined = (
                0.38 * recall_at_5
                + 0.26 * mrr
                + 0.12 * symbol_recall_at_5
                + 0.10 * byte_score
                + 0.08 * latency_score
                + 0.06 * diversity
            )

        return {
            "combined_score": float(round(combined, 6)),
            "score": float(round(combined, 6)),
            "recall_at_5": float(round(recall_at_5, 6)),
            "symbol_recall_at_5": float(round(symbol_recall_at_5, 6)),
            "mrr": float(round(mrr, 6)),
            "packet_bytes": float(bytes_used),
            "packet_count": float(packet_count),
            "query_latency_ms": float(round(elapsed_ms, 6)),
            "candidate_diversity": float(round(diversity, 6)),
            "claimability_violations": float(claimability_violations),
            "unsupported_claim_violations": float(unsupported_claim_violations),
            "missing_required_count": float(len(missing_required)),
        }
    except Exception as exc:
        return {
            "combined_score": 0.0,
            "score": 0.0,
            "error": str(exc),
            "claimability_violations": 0.0,
            "unsupported_claim_violations": 0.0,
        }


def main() -> int:
    parser = argparse.ArgumentParser(description="Evaluate a candidate-spool policy module")
    parser.add_argument("program_path")
    parser.add_argument("--output", default=None)
    args = parser.parse_args()

    metrics = evaluate(args.program_path)
    text = json.dumps(metrics, indent=2, sort_keys=True)
    print(text)
    if args.output:
        Path(args.output).parent.mkdir(parents=True, exist_ok=True)
        Path(args.output).write_text(text + "\n", encoding="utf-8")
    return 0 if metrics.get("combined_score", 0.0) > 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())

