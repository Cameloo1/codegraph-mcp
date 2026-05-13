# codegraph-ui

Phase 30 static Proof-Path UI assets.

`codegraph-mcp serve-ui` embeds and serves the files in `static/` from a
loopback-only Rust HTTP server. The UI renders real CodeGraph proof path,
neighborhood, impact, auth/security, event-flow, test-impact, unresolved-call,
source-span, and context-packet JSON from the local `.codegraph/codegraph.sqlite`
store. D3.js is vendored locally under `static/`; the UI does not use CDN assets
or mock graph data.
