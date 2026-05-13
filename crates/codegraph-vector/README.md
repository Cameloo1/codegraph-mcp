# codegraph-vector

Stage 1 binary-vector sieve and Stage 2 compressed rerank crate.

`MVP.md` is the source of truth for this crate's phase boundary and acceptance
criteria.

Phase 11 implemented bit-packed binary signatures, deterministic text-hash
signature generation, XOR + popcount Hamming distance, top-k local retrieval,
and exact-seed union so Stage 0 seeds cannot be dropped by the binary sieve.

Phase 12 adds `CompressedVectorReranker`, rerank candidate/score types,
placeholder int8/PQ/Matryoshka vector types, Matryoshka prefix validation, and a
deterministic local reranker that returns top-N candidates for later graph
verification.

Guardrail: no exact graph verification integration or external vector service
dependency lives here yet. FAISS and Qdrant adapters are feature-gated
placeholders only.
