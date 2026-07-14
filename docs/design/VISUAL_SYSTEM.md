# CodeGraph visual system: Verified Signal Field

## Purpose

The visual system expresses the product contract without overstating it:

> vectors suggest; graph verifies; source spans support the packet

Candidate, text, vector, source-navigation, and unresolved evidence must remain visually distinct from graph/source-verified proof. Unknown and unsupported states stay visible as unknown.

## Core metaphor

Repository evidence enters as many candidate trajectories. Paths converge at a small verification point. Unsupported trajectories terminate, fade, or remain dashed. Verified paths continue as compact, source-spanned proof bundles.

## Palette

| Token | Value | Use |
|---|---|---|
| void | `#030508` | page and graph background |
| ink | `#070a10` | secondary background |
| surface | `#0c1017` | controls and drawers |
| divider | `#202630` | rules and boundaries |
| text | `#f4f6f8` | primary text |
| muted | `#aeb7c4` | secondary text |
| dim | `#768292` | telemetry and inactive labels |
| blue | `#1e8bff` | parser-verified paths |
| cyan | `#1bc8e5` | LSP/selected route emphasis |
| mint | `#17d9a1` | exact/compiler-verified proof |
| violet | `#8b78f6` | dynamic or derived paths |
| magenta | `#d86bf2` | heuristic evidence |
| amber | `#e7b85a` | warning/lifecycle attention |
| rose | `#f1788e` | inferred/blocking/integrity failure |

Color is concentrated in graph linework. Large surfaces remain near-black. Avoid broad purple mesh gradients, glass panels, rounded-card grids, and decorative status colors.

## Exactness grammar

- `exact`, `compiler_verified`: mint, solid
- `lsp_verified`: cyan, solid
- `parser_verified`: blue, solid
- `dynamic_trace`: violet, solid
- `derived_from_verified_edges`: cyan/violet double or parallel line
- `static_heuristic`: magenta, dashed
- `inferred`: rose, dotted
- candidate/text/vector/source-navigation evidence: neutral gray-blue, dashed, explicitly outside proof

Hue is never the only signal. Stroke pattern and text labels preserve meaning for color-vision differences.

## Geometry

- Use deterministic layered placement rather than force-directed randomness.
- Use thin cubic paths with stable, ID-derived offsets.
- Normal nodes are 3-5 px beads with 32-44 px invisible hit areas.
- Selected nodes use an 8 px bead and a restrained ring.
- Show arrow direction only on the selected path.
- Show labels on selected or structurally important paths, not every edge.
- Partial circles, axes, and boundary bands must correspond to traversal depth, source/target levels, test lanes, or trust boundaries.

## Typography

Use a restrained grotesk for display and body text and a mono face for code, telemetry, exactness, and source spans. Display headlines stay within two lines. Avoid all-caps labels above every section and avoid random serif emphasis.

## Shapes and surfaces

- Section and canvas corners: square
- Drawers and panels: 0-2 px
- Inputs and buttons: 5-6 px
- Status beads: circular
- Hierarchy comes from dividers, spacing, and path density, not shadows or nested cards

## Modes

- Proof path: one emphasized left-to-right bundle
- Neighborhood: radial fan from selected entity
- Impact: diverging downstream cascade
- Auth/security: trust-boundary bands and crossing paths
- Event flow: parallel wave bundles
- Test impact: production lane above, test/mock lane below
- Unresolved calls: interrupted dashed paths with open endpoints
- Compare: mirrored fields with a shared center axis

## States

- Loading: neutral paths draw toward an inactive verification point
- No proof: candidate paths stop before the verification boundary
- Missing/stale DB: locked boundary with exact DB path and recovery command
- Truncated: visibly clipped paths plus omitted counts
- Error: preserve query state and show the failure inline

## Claims discipline

Do not visually or verbally imply that CodeGraph replaces tests, replaces literal search, prevents all hallucinations, understands every runtime behavior, or offers equal proof depth across languages. Local measurements remain local examples, not external benchmark claims.
