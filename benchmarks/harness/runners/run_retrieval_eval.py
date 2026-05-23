from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

from benchmarks.harness.adapters.crosscodeeval_adapter import CrossCodeEvalAdapter
from benchmarks.harness.adapters.internal_gold_adapter import InternalGoldAdapter
from benchmarks.harness.adapters.repobench_adapter import RepoBenchAdapter
from benchmarks.harness.config import BenchmarkConfig, load_config
from benchmarks.harness.context_providers.base import ContextProvider, ProviderBudget
from benchmarks.harness.context_providers.codegraph_exact_text import CodeGraphExactTextProvider
from benchmarks.harness.context_providers.codegraph_full import CodeGraphFullProvider
from benchmarks.harness.context_providers.codegraph_planned import CodeGraphPlannedProvider
from benchmarks.harness.context_providers.none import NoneProvider
from benchmarks.harness.context_providers.rg_only import RgOnlyProvider
from benchmarks.harness.context_providers.rg_planned import RgPlannedProvider
from benchmarks.harness.schema import new_result, validate_result
from benchmarks.harness.scoring.claimability import claimability_violations, no_proof_behavior_ok
from benchmarks.harness.scoring.context_recall import score_retrieval
from benchmarks.harness.scoring.efficiency import aggregate_by_mode
from benchmarks.harness.scoring.hallucination import unsupported_claim_violations
from benchmarks.harness.paths import results_path
from benchmarks.harness.task_sanitizer import sanitize_provider_task
from benchmarks.harness.workspace import ensure_dir, stable_id


PROVIDERS = {
    "none": NoneProvider,
    "baseline": NoneProvider,
    "rg_only": RgOnlyProvider,
    "rg_planned": RgPlannedProvider,
    "codegraph_exact_text": CodeGraphExactTextProvider,
    "codegraph_full": CodeGraphFullProvider,
    "codegraph_current": CodeGraphFullProvider,
    "codegraph_planned": CodeGraphPlannedProvider,
}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", default="benchmarks/tracks/internal_gold/configs/smoke.toml")
    parser.add_argument("--output-dir", default=None)
    parser.add_argument("--modes", nargs="*", default=None)
    parser.add_argument("--max-tasks", type=int, default=None)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    config = load_config(args.config)
    output_dir = Path(args.output_dir) if args.output_dir else _default_output_dir(config)
    results = run(config, output_dir, modes=args.modes, max_tasks=args.max_tasks, dry_run=args.dry_run)
    errors = [error for result in results for error in validate_result(result)]
    leakage_failures = [
        result
        for result in results
        if not result.get("trust", {}).get("leakage_audit", {}).get("pass", True)
    ]
    return 1 if errors or leakage_failures else 0


def run(
    config: BenchmarkConfig,
    output_dir: Path,
    modes: list[str] | None = None,
    max_tasks: int | None = None,
    dry_run: bool = False,
) -> list[dict]:
    ensure_dir(output_dir)
    ensure_dir(output_dir / "per_task")
    run_id = f"{config.name}-{int(time.time())}-{stable_id(str(output_dir))}"
    adapter = _adapter(config)
    tasks = adapter.load_tasks(limit=max_tasks or config.max_tasks)
    selected_modes = modes or config.modes
    budget = ProviderBudget(
        max_time_s=config.max_time_s,
        max_tool_calls=config.max_tool_calls,
        max_context_bytes=config.max_context_bytes,
    )
    results: list[dict] = []
    for mode in selected_modes:
        provider = _provider(mode, config)
        for task in tasks:
            provider_task = sanitize_provider_task(
                {
                    **task,
                    "max_output_bytes": config.max_output_bytes,
                    "max_tool_calls": config.max_tool_calls,
                    "max_wall_time_ms": (config.max_time_s * 1000 if config.max_time_s else None),
                }
            )
            leakage_audit = provider_task["leakage_audit"]
            start = time.perf_counter()
            packet = provider.get_context(provider_task, budget) if not dry_run else provider.metadata()
            if not isinstance(packet, dict):
                packet_dict = packet.to_dict()
            else:
                packet_dict = packet
            wall = int((time.perf_counter() - start) * 1000)
            retrieval = score_retrieval(task, packet_dict)
            claim_errors = claimability_violations(packet_dict)
            unsupported = unsupported_claim_violations(packet_dict)
            result = new_result(
                run_id=run_id,
                task_id=task["task_id"],
                benchmark=_benchmark_name(config),
                track=_track_name(config, dry_run),
                mode=mode if mode != "none" else "baseline",
                model="none",
                agent_scaffold="retrieval_only",
                context_provider=mode,
                repo=task.get("repo_path", ""),
                repo_commit=task.get("repo_commit", ""),
                dataset_version=config.dataset_version or adapter.dataset_version,
                budget={
                    "max_time_s": config.max_time_s,
                    "max_tool_calls": config.max_tool_calls,
                    "max_input_tokens": None,
                    "max_output_tokens": None,
                },
                retrieval=retrieval,
                efficiency={
                    "wall_time_ms": wall,
                    "tool_calls": packet_dict.get("tool_calls", 0),
                    "codegraph_calls": packet_dict.get("tool_calls", 0) if mode.startswith("codegraph") else 0,
                    "rg_calls": packet_dict.get("tool_calls", 0) if mode in {"rg_only", "rg_planned"} else 0,
                    "output_lines": retrieval.get("output_lines"),
                    "cold_setup_time_ms": _provider_timing(packet_dict).get("cold_setup_time_ms"),
                    "warm_retrieval_time_ms": _provider_timing(packet_dict).get("warm_retrieval_time_ms"),
                    "codegraph_query_subprocess_ms": _provider_timing(packet_dict).get("codegraph_query_subprocess_ms"),
                    "codegraph_context_pack_subprocess_ms": _provider_timing(packet_dict).get("codegraph_context_pack_subprocess_ms"),
                    "codegraph_index_subprocess_ms": _provider_timing(packet_dict).get("codegraph_index_subprocess_ms"),
                    "cost_usd": None,
                },
                trust={
                    "claimability_violations": len(claim_errors),
                    "unsupported_claim_violations": unsupported,
                    "no_proof_behavior_ok": no_proof_behavior_ok(task, packet_dict),
                    "stale_db_blocked": None,
                    "leakage_audit": leakage_audit,
                    "query_leakage_violations": 0 if leakage_audit.get("pass") else 1,
                    "evidence_alignment": retrieval.get("evidence_alignment"),
                    "routing_packet_usefulness": retrieval.get("routing_packet_usefulness"),
                    "implementation_trace_usefulness": retrieval.get("implementation_trace_usefulness"),
                    "artifact_db_inspection_requirement_correct": retrieval.get("artifact_db_inspection_requirement_correct"),
                },
                artifacts={"raw_log": "", "prediction": None, "patch": None, "context_packet": None, "provider_input": None},
            )
            task_file_id = stable_id(f"{task['task_id']}:{mode}")
            packet_path = output_dir / "per_task" / f"{task_file_id}_context.json"
            packet_path.write_text(json.dumps(packet_dict, indent=2), encoding="utf-8")
            result["artifacts"]["context_packet"] = str(packet_path)
            provider_input_path = output_dir / "per_task" / f"{task_file_id}_provider_input.json"
            provider_input_path.write_text(json.dumps(provider_task, indent=2), encoding="utf-8")
            result["artifacts"]["provider_input"] = str(provider_input_path)
            result_path = output_dir / "per_task" / f"{task_file_id}.json"
            result_path.write_text(json.dumps(result, indent=2), encoding="utf-8")
            results.append(result)
    summary = {
        "schema_version": "benchmark_summary_v1",
        "run_id": run_id,
        "result_kind": "clean_retrieval_diagnostic",
        "clean_retrieval_comparable": True,
        "public_claim": False,
        "leakage_audit_pass": all(
            result.get("trust", {}).get("leakage_audit", {}).get("pass", True) for result in results
        ),
        "modes": aggregate_by_mode(results),
    }
    (output_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    _write_markdown(output_dir / "summary.md", summary)
    return results


def _provider(mode: str, config: BenchmarkConfig) -> ContextProvider:
    cls = PROVIDERS[mode]
    return cls(Path.cwd(), config.workspace_dir, config.release_binary)


def _provider_timing(packet: dict) -> dict:
    raw = packet.get("raw") if isinstance(packet.get("raw"), dict) else {}
    timing = raw.get("codegraph_subprocess_timing_ms") if isinstance(raw.get("codegraph_subprocess_timing_ms"), dict) else {}
    query_ms = int(timing.get("query_subprocess_ms") or 0)
    context_ms = int(timing.get("context_pack_subprocess_ms") or 0)
    index_ms = int(timing.get("index_prebuild_ms") or 0)
    return {
        "cold_setup_time_ms": index_ms,
        "warm_retrieval_time_ms": query_ms + context_ms,
        "codegraph_query_subprocess_ms": query_ms,
        "codegraph_context_pack_subprocess_ms": context_ms,
        "codegraph_index_subprocess_ms": index_ms,
    }


def _adapter(config: BenchmarkConfig):
    if config.dataset == "internal_gold":
        return InternalGoldAdapter(config.dataset_path)
    if config.dataset == "repobench":
        language = str(config.raw.get("adapter", {}).get("language", "python"))
        return RepoBenchAdapter(config.dataset_path, language=language)
    if config.dataset == "crosscodeeval":
        languages = config.raw.get("adapter", {}).get("languages")
        return CrossCodeEvalAdapter(config.dataset_path, languages=list(languages) if isinstance(languages, list) else None)
    raise ValueError(f"retrieval runner does not support dataset: {config.dataset}")


def _benchmark_name(config: BenchmarkConfig) -> str:
    if config.dataset == "internal_gold":
        return "internal"
    return config.dataset


def _default_output_dir(config: BenchmarkConfig) -> Path:
    track_by_dataset = {
        "internal_gold": "internal_gold",
        "repobench": "repobench",
        "crosscodeeval": "crosscodeeval",
        "swe_bench_lite": "swebench_lite",
    }
    track_id = track_by_dataset.get(config.dataset)
    if track_id is None:
        return Path("benchmarks/results/summaries") / f"{int(time.time())}_{config.name}"
    return results_path(track_id, f"{int(time.time())}_{config.name}")


def _track_name(config: BenchmarkConfig, dry_run: bool) -> str:
    if dry_run:
        return "diagnostic"
    if config.dataset == "repobench":
        return "product_ablation_diagnostic"
    if config.dataset == "crosscodeeval":
        return "retrieval_context_diagnostic"
    return "product_ablation"


def _write_markdown(path: Path, summary: dict) -> None:
    lines = [
        "# Benchmark Summary",
        "",
        "Diagnostic/local result. Not a public benchmark claim.",
        "",
        f"Leakage audit pass: {summary.get('leakage_audit_pass')}",
        "",
        "| Mode | Tasks | Recall@5 | MRR | Precision@5 | Wrong context@5 | Poison count | Context Bytes | Tool Calls | Claim Violations |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for mode, metrics in summary["modes"].items():
        lines.append(
            "| {mode} | {tasks} | {recall} | {mrr} | {precision} | {wrong} | {poison} | {bytes} | {calls} | {violations} |".format(
                mode=mode,
                tasks=metrics.get("tasks"),
                recall=_fmt(metrics.get("gold_file_recall_at_5")),
                mrr=_fmt(metrics.get("mrr")),
                precision=_fmt(metrics.get("precision_at_5")),
                wrong=_fmt(metrics.get("wrong_context_rate_at_5")),
                poison=_fmt(metrics.get("context_poison_count")),
                bytes=_fmt(metrics.get("context_bytes")),
                calls=_fmt(metrics.get("tool_calls")),
                violations=metrics.get("claimability_violations"),
            )
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def _fmt(value) -> str:
    if value is None:
        return "unknown"
    return f"{float(value):.3f}"


if __name__ == "__main__":
    raise SystemExit(main())
