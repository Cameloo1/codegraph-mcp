# Graph Visual Grammar

This document maps CodeGraph evidence fields to visual properties. It applies to the local Proof-Path UI, product-site illustrations, README diagrams, and future documentation assets.

## Semantic hierarchy

Visual precedence follows the product evidence boundary:

1. graph/source-supported path;
2. selected graph entity or edge;
3. source-text or candidate evidence;
4. warning, unresolved, or unknown evidence;
5. guides, axes, and decorative telemetry.

A lower-precedence layer must not become visually stronger than the selected proof path.

## Exactness

| Exactness | Hue | Stroke | Notes |
| --- | --- | --- | --- |
| `exact` | mint | solid | strongest verified state |
| `compiler_verified` | mint | solid | may use a brighter endpoint |
| `lsp_verified` | cyan | solid | future or available LSP-backed evidence |
| `parser_verified` | electric blue | solid | normal parser-backed graph relation |
| `dynamic_trace` | violet | solid | runtime trace evidence when present |
| `derived_from_verified_edges` | cyan | double or compound dash | must retain provenance |
| `static_heuristic` | magenta | dashed | not exact graph proof |
| `inferred` | rose | dotted | weakest relation-like evidence |

Color is never the only channel. Stroke pattern and text labels must remain available.

## Candidate and text evidence

Candidate-only, vector, lexical, binary, nuance-rescue, source-navigation, and source-text fallback evidence use neutral blue-gray or restrained blue with a dash pattern.

Required labels include one of:

- `candidate`;
- `text_evidence`;
- `source_navigation`;
- `no_proof_path_found`;
- the actual evidence role returned by the product surface.

They must not use the mint verified treatment.

## Confidence

Confidence controls opacity and, within a narrow range, stroke width.

Recommended mapping:

```text
opacity = 0.28 + confidence * 0.62
stroke  = 0.9 px + confidence * 1.45 px
```

Do not use confidence to change exactness hue or promote a dashed path to solid.

## Nodes

Normal visible bead:

- radius: 3–5 px;
- stroke: exactness or selected-path hue;
- fill: dark blue-black.

High-degree or source/target bead:

- radius: 5–7 px;
- slightly stronger fill and stroke.

Selected bead:

- radius: 7–9 px;
- white or near-white fill;
- cyan ring or dashed orbit.

Interaction target:

- radius or box equivalent: at least 20 px around the bead;
- transparent;
- keyboard focusable when interactive.

Do not return to 28 px visible bubbles for normal entities.

## Edge geometry

The local UI remains deterministic. Use layered DAG positions and route edges as cubic Bézier paths.

Recommended process:

1. keep stable source and target levels;
2. group edges by source level and target level;
3. compute a shared approximate bundle center;
4. route each edge through the center with a small stable offset derived from edge ID;
5. keep topology readable and labels out of high-density intersections.

Do not use an unconstrained force simulation for the default proof view.

## Direction

Avoid permanent arrowheads on every line.

Use one or more of:

- a small directional bead on the selected path;
- a compact chevron near the selected edge midpoint;
- relation text in reading order;
- source and target labels;
- animated direction only during active inspection.

## Labels

Default graph labels:

- show source and target labels;
- show high-degree nodes;
- show selected path relations;
- hide ordinary node and edge labels until hover, focus, or selection.

Use an opaque text halo matching the graph background so labels remain readable over paths.

Never print fake decimals or IDs solely to imitate telemetry.

## Guides

Guides may encode:

- traversal level;
- source or target depth;
- verification boundary;
- production versus test lane;
- trust boundary in security mode;
- omitted/truncated boundary.

Guides should use 5–15% opacity and must remain below evidence paths.

## Mode-specific composition

| Mode | Default composition |
| --- | --- |
| Proof path | one emphasized left-to-right route; secondary paths dimmed |
| Neighborhood | radial or fan arrangement around the selected entity |
| Impact | diverging downstream cascade |
| Auth/security | explicit trust-boundary bands and crossing paths |
| Event flow | parallel wave or lane bundles around event/topic nodes |
| Test impact | production lane above, test/mock lane below |
| Unresolved calls | dashed paths ending at open endpoints |
| Compare | mirrored route fields around a shared axis |

## States

### Loading

Draw a small neutral set of paths toward an inactive verification point. Avoid a generic spinner when the graph stage is visible.

### Empty

Keep source and target context visible. Use interrupted paths and state the reason precisely.

### Stale or unsafe lifecycle

Place a visible boundary between the user and the graph. Show the checked DB path, lifecycle reason, and recovery command where available.

### Truncated

Clip paths at a visible boundary and state omitted node and edge counts.

### Request failure

Keep the query intact and show the error in the evidence inspector. Do not replace the application with an alert dialog.

## Accessibility

- Color must be redundant with pattern or text.
- Interactive paths and nodes require large invisible hit areas.
- Focus must be visible.
- The graph requires an accessible name.
- Source and relation details remain available as text in the inspector.
- Reduced-motion mode disables path drawing, perpetual direction particles, and parallax.
