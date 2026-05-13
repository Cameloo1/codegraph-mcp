# codegraph-query

Exact graph query crate.

`MVP.md` is the source of truth for this crate's phase boundary and acceptance
criteria.

Phases 08 through 14 implement `ExactGraphQueryEngine` over extracted `Edge`
facts, graph-only context packet construction, the integrated retrieval funnel,
and Bayesian uncertainty ranking. The query layer supports caller/callee,
read/write/mutation, dataflow, auth, event, migration, test, trace-path, core
impact queries, PathEvidence generation, explainable DerivedClosureEdge
generation, source-span snippet extraction, risk summaries, recommended test
selection, prompt seed extraction, and exact seed preservation in
`context_pack`.

Phase 13 added `RetrievalFunnel`, which composes Stage 0 exact seeds, Stage
1 binary sieve candidates, Stage 2 compressed reranking, Stage 3 exact graph
verification, and Stage 4 context packet emission with structured trace output.

Phase 14 adds deterministic Bayesian/logistic scoring over graph-verified
funnel paths, configurable weights, relation reliability priors, uncertainty
penalties, packet confidence/uncertainty metadata, and calibration placeholders
for Brier score and reliability buckets.

Guardrail: MCP behavior, UI behavior, and benchmark execution live in their
own crates. Post-MVP RL/autonomous optimization is not implemented here.
