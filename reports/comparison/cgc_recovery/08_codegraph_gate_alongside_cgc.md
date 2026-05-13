# 08 CodeGraph Gate Alongside CGC

Generated: 2026-05-13 08:51:00 -05:00

- cargo test --workspace: pass
- Graph Truth Gate: 11/11, verdict passed
- Context Packet Gate: 11/11, verdict passed
- DB integrity: ok
- Fresh comprehensive verdict: fail
- Fresh proof build: 184297 ms, target <=60,000 ms
- Proof DB: 171.184 MiB
- Repeat unchanged: 1674.0 ms
- Single-file update: 336.0 ms

Official comparison allowed: false.

The current CodeGraph-side blocker is the fresh proof-build-only measurement. CGC was run diagnostically, but no official superiority claim is allowed.