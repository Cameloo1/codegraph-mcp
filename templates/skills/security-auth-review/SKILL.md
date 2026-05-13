---
name: security-auth-review
description: Use CodeGraph auth and security paths to review authorization, role checks, sanitization, validation, and exposed routes.
---

# Security Auth Review

Use this skill when reviewing auth, permissions, role checks, trust boundaries, sanitizers, validators, taint sources, or exposed handlers.

Rules:
1. Read `MVP.md` first if it is available; treat it as the source of truth.
2. Do not use subagents.
3. Call `codegraph.context_pack` before non-trivial edits.
4. Prefer verified relation paths over semantic similarity.
5. Use exact source spans returned by CodeGraph when reading or editing code.
6. For auth/security tasks, call `codegraph.find_auth_paths`.
7. For mutation or persistence tasks, call `codegraph.find_mutations` or `codegraph.impact_analysis`.
8. After edits, call `codegraph.update_changed_files`.
9. Run recommended tests when practical.

Workflow:
1. Request context for the route, handler, policy, role, permission, or security finding.
2. Use `codegraph.find_auth_paths` to connect EXPOSES, AUTHORIZES, CHECKS_ROLE, SANITIZES, and VALIDATES evidence.
3. Treat heuristic security edges as leads until source spans confirm behavior.
4. Re-index changed files and run the packet's recommended security or route tests.
