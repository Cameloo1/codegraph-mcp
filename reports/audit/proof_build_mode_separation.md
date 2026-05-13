# Proof Build Mode Separation

Source of truth: `MVP.md`.

Generated: 2026-05-13 00:56:13 -05:00

## Verdict

Mode separation is enforced. `proof_build_only_ms` now measures the production proof DB build path, not validation/audit/reporting/CGC work.

## Modes

- `codegraph-mcp bench proof-build-only`: production proof DB build only.
- `codegraph-mcp bench proof-build-validated`: proof build plus validation.
- `codegraph-mcp bench comprehensive`: full master gate.
- `codegraph-mcp bench cgc-comparison`: gated competitor comparison, never automatic from proof-build-only.

## Latest Separation

- Proof-build-only: 59414 ms
- Validation: separate
- Audit/report generation: separate
- CGC comparison: separate, not run by proof-build-only
