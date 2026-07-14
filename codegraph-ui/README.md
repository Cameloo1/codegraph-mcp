# codegraph-ui

Bundled static assets for the local CodeGraph Proof-Path UI.

`codegraph-mcp serve-ui` embeds and serves `static/` from a loopback-only Rust
HTTP server. The UI reads real CodeGraph proof path, neighborhood, impact,
auth/security, event-flow, test-impact, unresolved-call, source-span, and
context-packet JSON from the configured local graph store. D3 is vendored under
`static/`; the UI does not use CDN assets or mock graph data.

## Verified Signal Field redesign

The interface is a canvas-first investigation instrument rather than a generic
three-column dashboard:

- the graph remains the dominant surface;
- source, target, mode, and relation controls share one query stage;
- relation families live in a collapsible drawer;
- evidence details and source spans live in an inspector;
- layered DAG positions remain deterministic;
- visible nodes are small beads with larger invisible pointer targets;
- edges are cubic bundled paths rather than straight lines;
- exactness uses hue and stroke pattern;
- confidence affects opacity and a narrow stroke-width range;
- selected paths can be isolated without hiding source evidence;
- mobile relation and evidence panels become sheets;
- reduced-motion mode removes non-essential animation.

The redesign preserves the existing local endpoints and graph JSON contract.
See `docs/design/VISUAL_SYSTEM.md` and
`docs/design/GRAPH_VISUAL_GRAMMAR.md` for the shared visual rules.

## Local use

```bash
codegraph-mcp serve-ui --port 7878
```

Open `http://127.0.0.1:7878` after indexing the repository. The server remains
loopback-only by default and keeps the existing same-origin content security
policy.
