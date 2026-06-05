# Source Span Exact Callsite

This fixture catches proof paths that resolve the correct callee but attach the
wrong source span. `run` calls `first()` and then `second()` nearby; the expected
`CALLS` edge to `second` must point at the exact `second()` callsite, not the
adjacent `first()` call or the whole containing function.

The case is a graph-truth fixture only. It is not benchmark-score evidence and
does not make any public performance claim.
