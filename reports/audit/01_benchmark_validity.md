# 01 Benchmark Validity Audit

Verdict: not trustworthy for graph correctness claims.

The current benchmark suite is useful as smoke coverage and artifact plumbing, but it cannot yet prove the MVP claim that exact graph/source verification is correct. Several benchmark paths use implementation-shaped synthetic fixtures, feed expected symbols into retrieval seeds, lack forbidden cases, and score relation/path correctness weakly. The latest reports are therefore partially useful for latency/storage and command behavior, but not trustworthy as proof of graph correctness.

## Sources Inspected

- `MVP.md`
- `README.md`
- `docs/benchmark-guide.md`
- `docs/quality-gates.md`
- `crates/codegraph-bench/README.md`
- `crates/codegraph-bench/src/lib.rs`
- `crates/codegraph-bench/src/two_layer.rs`
- `crates/codegraph-bench/src/competitors/codegraphcontext.rs`
- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`
- `target/codegraph-bench-report/sweep-20260509-231456/human-report-data.json`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/run.json`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/status.json`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/dbstat.json`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/query-latencies.json`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-cgc/run.json`
- `target/codegraph-bench-runs/sweep-20260509-231456-retrieval-quality/summary.md`
- `target/codegraph-bench-runs/sweep-20260509-231456-retrieval-quality/manifest.json`
- `target/codegraph-bench-runs/sweep-20260509-231456-agent-quality/summary.md`
- `target/codegraph-bench-runs/sweep-20260509-231456-agent-quality/manifest.json`
- `target/cgc-comparison-full/summary.md`
- `target/cgc-comparison-full/run.json`

## Benchmark Inventory

| Benchmark surface | Code and artifacts | What it measures | Ground truth present | Current verdict |
| --- | --- | --- | --- | --- |
| Internal baseline suite | `crates/codegraph-bench/src/lib.rs`, `codegraph-mcp bench` | Command success, file recall, symbol recall, relation substring matches, path relation-sequence recall, latency/token/memory estimates | Synthetic expected relations/files/symbols/tests | Not trustworthy for graph correctness because expected symbols/tests are reused as graph seeds. |
| Fixture task families | `relation_extraction_repo`, `long_chain_repo`, `context_retrieval_repo`, `agent_patch_repo`, `compression_repo`, `security_auth_repo`, `async_event_repo`, `test_impact_repo` in `crates/codegraph-bench/src/lib.rs` | Happy-path controlled TS fixtures across relation/path/context/test families | Expected files, symbols, relations, relation sequences, tests | Partially trustworthy as smoke fixtures only. No forbidden edges/paths or distractors. |
| Two-layer retrieval-quality | `crates/codegraph-bench/src/two_layer.rs`, `target/codegraph-bench-runs/sweep-20260509-231456-retrieval-quality` | Artifact production, per-task normalized outputs, baseline/CodeGraph/CGC mode bookkeeping | Reuses internal task ground truth and real-repo manifests | Not trustworthy. The latest run reports 7 losses, 1 tie, 1 unknown, 0 wins; metric normalization also has a recall fallback problem. |
| Two-layer agent-quality dry run | `crates/codegraph-bench/src/two_layer.rs`, `target/codegraph-bench-runs/sweep-20260509-231456-agent-quality` | Fake agent trace/artifact shape, evidence-use scoring, patch path creation | Expected patch/test fields in synthetic task | Not trustworthy for agent patch success. Hidden/build/visible test pass are explicitly unknown. |
| Autoresearch full repo index | `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/*` | Full-repo indexing time, DB size, counts, query latencies | No graph-correctness labels | Trustworthy only for observed storage/latency on that run. It does not prove edge correctness. |
| CGC comparison harness | `crates/codegraph-bench/src/competitors/codegraphcontext.rs`, `target/cgc-comparison-full/*` | CodeGraph vs CGC fixture output normalization and aggregate recall/F1 | External fixture expected files/symbols/relation sequences/path symbols | Not trustworthy as a fair correctness comparison until query and parsing equivalence are validated. |
| Gap scoreboard | `write_gap_scoreboard_report` in `crates/codegraph-bench/src/lib.rs`, `codegraph-mcp bench gaps` | Win/loss/tie/unknown dimensions | Internal aggregates plus optional CGC aggregate | Useful for known/unknown bookkeeping, not graph truth. |
| Final acceptance gate | `write_final_acceptance_gate_report` in `crates/codegraph-bench/src/lib.rs`, `codegraph-mcp bench final-gate` | Compact fixture acceptance, storage policy, CLI/MCP parity, CGC status | Internal compact fixture checks | Useful as a release gate, not sufficient as manually labeled graph truth. |

## What Current Benchmarks Prove

- The Rust workspace has benchmark entrypoints and report writers.
- Synthetic repos can be generated, indexed, queried, and serialized.
- The latest Autoresearch CodeGraph index completed in 169950 ms and produced a large SQLite DB with 865896 entities, 2050123 edges, and 2916019 source spans.
- Storage/latency artifacts identify real operational risks: an 803495936 byte DB file and an `unresolved-calls` query at 71271 ms.
- CGC timeout and skip/error states can be represented without fabricating data.

## What Current Benchmarks Do Not Prove

- They do not prove manually labeled graph correctness.
- They do not prove source-span correctness beyond presence/counts.
- They do not prove edge endpoints are semantically correct.
- They do not prove path correctness as ordered, endpoint-specific edge chains.
- They do not prove base facts are separated from derived or heuristic facts.
- They do not prove test/mock facts cannot contaminate production proof paths.
- They do not prove an agent can successfully patch real code.

## Ground Truth Coverage

| Requirement | Present today | Evidence |
| --- | --- | --- |
| Manually labeled ground truth | Mostly no | Synthetic fixtures are code-defined in `crates/codegraph-bench/src/lib.rs`; real-repo manifests mark many tasks as `unvalidated_expected`. |
| Expected entities | Partial | `GroundTruth.expected_symbols` exists, but not stable expected entity IDs/kinds/source spans. |
| Expected edges | Partial | `ExpectedRelation` has relation plus head/tail substring and optional file path, not exact IDs/spans/exactness. |
| Forbidden edges | No | No `forbidden_edges` field in `GroundTruth`. |
| Expected paths | Partial | `expected_relation_sequences` checks only relation sequence containment. |
| Forbidden paths | No | No `forbidden_paths` field in `GroundTruth`. |
| Expected source spans | No for internal suite | CGC external truth has `critical_source_spans`; internal `GroundTruth` does not. |
| Expected context symbols | Partial | Expected symbols double as retrieval seeds in graph baselines. |
| Expected tests | Partial | `expected_tests` exists but scoring only checks that some expected label appears. |
| Negative/distractor cases | Mostly no | Synthetic fixtures are positive and compact. The latest two-layer summary still reports no retrieval-quality wins. |

## Suspected Leakage and Weak Scoring

1. `crates/codegraph-bench/src/lib.rs::resolve_seed_ids` adds `task.ground_truth.expected_symbols` and `expected_tests` to graph seeds. This leaks expected answers into graph-only, graph+binary/PQ, graph+Bayesian, and full context packet baselines.
2. `ExpectedRelation` matches by `head_id.contains(...)` and `tail_id.contains(...)`, so broad or accidental IDs can pass without exact endpoint proof.
3. `path_recall_at_k` accepts relation-sequence containment. A path can match `CALLS, MUTATES` without proving the expected source, target, intermediate symbols, or source spans.
4. `evaluate_result` computes patch success as expected patch success plus recall >= 0.5. That is not actual patch execution.
5. `evaluate_result` computes test success as expected test success plus any observed expected test. That is not test execution.
6. `crates/codegraph-bench/src/two_layer.rs` reads `file_recall_at_k` using key `"10"` while internal metrics store keys like `"recall@10"`. The fallback then calls recall with an empty expected set, which returns 1.0. That can make normalized retrieval-quality recall meaningless.
7. `fake_agent_record` hardcodes evidence-use scores and writes a fake patch/final answer while hidden/build/visible tests remain `unknown`.
8. The internal suite has no false-positive penalty for extra files/symbols unless they happen to reduce precision in the broad artifact set. There is no forbidden edge/path list.
9. Latest `target/cgc-comparison-full/summary.md` shows CodeGraph modes with `false_proof_count` 14 and 17. Current aggregate tables still look competitive despite explicit false proofs.

## CGC Harness Audit

CGC fixture evidence recall appears to normalize to 0 because the harness only credits conservative file, symbol, path-symbol, relation, and source-span evidence parsed from CGC stdout/stderr. When CGC emits human-readable text without repo-relative file paths or relation labels, the normalizer records no file/path/relation evidence. In `target/cgc-comparison-full/run.json`, the `test-impact` CGC normalized output contains symbols like `Deleted`, `old`, `index`, and `calculateTotal`, but no files, paths, relations, or source spans.

The harness may be unfair to CGC in several ways:

- CGC is queried through multiple guessed command forms and stdout parsing, while CodeGraph is evaluated through internal Rust structures converted to normalized JSON.
- CGC `call_chain` source/target are selected from expected path symbols, not necessarily from a CGC-native query contract.
- CodeGraph fixtures are converted to internal `BenchmarkTask` values from the same ground truth used for scoring.
- File evidence is required in normalized CGC output, but CGC may index successfully while reporting results in a different shape.

The harness may also be unfair to CodeGraph in one way: CodeGraph context-packet mode is penalized when it intentionally returns compact snippets/symbols rather than full raw path output. That should be scored under a separate context-packet usefulness rubric.

Conclusion: CGC comparison is currently useful for harness plumbing and raw artifact collection, but not as a fairness-grade competitor result.

## Are Current Pass Results Meaningful?

Partially, for smoke behavior. They show commands run, outputs serialize, and some expected labels are retrieved. They are not meaningful as evidence that the graph is correct, proof-grade, or superior to CGC.

Current fixture results cannot prove graph correctness. They can only prove that implementation-generated fixtures still match implementation-shaped expectations.

## Files and Functions Needing Changes

- `crates/codegraph-bench/src/lib.rs::GroundTruth`: add exact expected entities, edges, paths, spans, context symbols, forbidden edges/paths, distractors, and scorer policy.
- `crates/codegraph-bench/src/lib.rs::resolve_seed_ids`: stop using expected outputs as retrieval seeds.
- `crates/codegraph-bench/src/lib.rs::relation_score`: match exact IDs/kinds/source spans/exactness/provenance, not substring endpoints.
- `crates/codegraph-bench/src/lib.rs::path_recall_at_k`: score ordered edge IDs or source/target/metapath/source-span tuples.
- `crates/codegraph-bench/src/lib.rs::evaluate_result`: remove simulated patch/test success from correctness metrics.
- `crates/codegraph-bench/src/two_layer.rs::retrieval_record`: fix metric-key mismatch and remove empty-ground-truth recall fallback.
- `crates/codegraph-bench/src/two_layer.rs::fake_agent_record`: keep dry-run fake-agent output out of quality claims.
- `crates/codegraph-bench/src/two_layer.rs::cgc_metrics`: add path/relation/source-span scoring or mark them unknown.
- `crates/codegraph-bench/src/competitors/codegraphcontext.rs::run_codegraphcontext_mode`: define a CGC-specific fair query contract and parse schema.
- `crates/codegraph-bench/src/competitors/codegraphcontext.rs::normalize_codegraphcontext_text`: keep unsupported/unparseable separate from wrong, and avoid treating boilerplate as symbols.
- `crates/codegraph-cli/src/lib.rs`: keep benchmark command reporting clear about unknowns and dry runs.

## Recommended Next Benchmark Schema

Create `benchmarks/graph_truth/schemas/graph_truth_task.schema.json` with at least:

- `task_id`, `query_text`, `query_seeds`: independent from expected outputs.
- `fixture_source`: manual label provenance and reviewer.
- `expected_entities`: stable ID, kind, name, qualified name, file, source span.
- `expected_edges`: head, relation, tail, source span, extractor class, exactness, confidence floor, production/test context.
- `forbidden_edges`: exact false positives that must fail.
- `expected_paths`: ordered edge references plus source/target and required source spans.
- `forbidden_paths`: paths that conflate test/mock/prod or heuristic/exact evidence.
- `expected_context_symbols` and `expected_context_snippets`.
- `expected_tests`: actual test command and expected pass/fail result, when applicable.
- `distractors`: files/symbols/relations deliberately similar to the correct answer.
- `scoring`: relation F1, path recall, source-span accuracy, false-proof count, and allowed unknowns.

The next benchmark build should start with a tiny manually labeled fixture that has one true path, one plausible false path, one test/mock-only path, and one unresolved heuristic call. That fixture should fail today until the scorer can reject the false positives.
