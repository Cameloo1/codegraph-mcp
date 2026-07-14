# CodeGraph Visual System

## Verified Signal Field

CodeGraph uses a dark editorial data-visualization system called **Verified Signal Field**.

The visual idea follows the product contract:

```text
candidate lanes
  -> union / deduplicate / rank
  -> graph and source verification
  -> proof path, source-text fallback, or explicit unknown
  -> compact context or validation packet
```

The system is not a generic neural-network aesthetic. It depicts repository evidence moving through a verification boundary.

## Design read

| Surface | Variance | Motion | Density |
| --- | ---: | ---: | ---: |
| Product site | 8 / 10 | 6 / 10 | 3 / 10 |
| README and docs | 7 / 10 | 2 / 10 | 4 / 10 |
| Proof-Path UI | 6 / 10 | 4 / 10 | 7 / 10 |

The product site is spacious and cinematic. The local UI is denser because it is an investigation instrument. Both use the same palette, graph semantics, typography hierarchy, and proof boundaries.

## Core visual principles

1. **Quiet field, concentrated signal.** Most of the surface remains near-black. Color belongs to paths, selected evidence, and small state indicators.
2. **The graph is the image.** Do not place a generic dashboard screenshot over a gradient. Use real graph geometry, real relation vocabulary, or deterministic procedural art.
3. **Candidate is not proof.** Candidate, text, vector, and unresolved evidence must be visually distinct from graph/source-supported paths.
4. **Small nodes, strong paths.** Normal nodes are beads rather than large bubbles. Interaction targets may be larger and invisible.
5. **Negative space is structural.** Headlines and evidence summaries occupy deliberately quiet regions rather than floating cards.
6. **No decorative precision.** Do not invent percentages, benchmark scores, latency, confidence, source lines, or user counts for visual effect.
7. **One restrained spectrum.** Blue, cyan, mint, violet, and magenta appear in graph linework. UI surfaces remain monochromatic.

## Tokens

```css
--cg-void: #030508;
--cg-ink: #070a10;
--cg-surface: #0c1017;
--cg-divider: #202630;
--cg-text: #f4f6f8;
--cg-text-muted: #b6bdc8;
--cg-text-dim: #7e8795;
--cg-blue: #1e8bff;
--cg-cyan: #1bc8e5;
--cg-mint: #17d9a1;
--cg-violet: #8b78f6;
--cg-magenta: #d86bf2;
--cg-amber: #e7b85a;
--cg-rose: #f1788e;
```

### Token usage

- `void` is the primary page and graph background.
- `ink` and `surface` separate controls or inspectors without creating a card wall.
- `divider` defines panels, axes, and section boundaries.
- `text`, `text-muted`, and `text-dim` provide three clear information levels.
- Chromatic tokens primarily encode graph evidence and state.

## Typography

Use a neutral grotesk with clear numeric and technical support. The CSS stack should work without remote font loading:

```css
font-family: "Geist", "Avenir Next", "Segoe UI", ui-sans-serif,
  system-ui, -apple-system, BlinkMacSystemFont, sans-serif;
```

Use a system monospace stack for commands, source spans, exactness, relation labels, and telemetry.

### Marketing scale

- Hero: 58–100 px depending on viewport, two lines maximum.
- Section heading: 40–80 px.
- Body: 15–20 px, approximately 60–65 characters wide.
- Telemetry: 8–12 px monospace.

### Product scale

- Panel heading: 15–18 px.
- Control text: 12–14 px.
- Source and graph labels: 8–12 px monospace.

Do not add an uppercase eyebrow above every section. Use one only when it communicates a real category or state.

## Shape language

- Graph and page sections: sharp or 2 px radius.
- Drawers and product panels: 0–3 px radius.
- Inputs and buttons: 5–6 px radius.
- Status dots and graph beads: circular.
- Do not introduce large rounded containers, universal pills, or heavy glassmorphism.

## Motion

Motion must communicate hierarchy, routing, verification, feedback, or state change.

Allowed:

- a selected path becoming brighter and wider;
- a directional bead moving along the selected path;
- candidate paths converging during the architecture sequence;
- drawers and inspectors entering from the relevant edge;
- restrained content reveal;
- zoom and pan feedback.

Avoid:

- perpetual motion on every graph line;
- random particle fields;
- spring effects unrelated to interaction;
- large glow pulses;
- scroll hijacking outside the single manifesto sequence.

All surfaces must work under `prefers-reduced-motion: reduce`.

## Surface applications

### README

The README uses:

- `docs/assets/readme/title-pic.svg` for the brand statement;
- `docs/assets/readme/agent_use_loop.svg` for the task loop;
- `docs/assets/readme/proof_taxonomy.svg` for the trust model;
- `docs/assets/readme/codegraph_terminal.svg` for a current illustrative command sequence.

### Product site

`site/` is a dependency-free static implementation of the full marketing narrative. It generates proof fields in the browser from deterministic seeds and makes no external network calls.

### Proof-Path UI

`codegraph-ui/static/` is a canvas-first local investigation interface. It retains the existing local endpoints and graph JSON contract while changing layout, edge routing, node scale, filters, and the evidence inspector.

## Asset generation

Generate promoted README SVGs from the repository root:

```bash
python scripts/brand/generate_proof_field.py --all
```

The output is deterministic for the committed presets in `scripts/brand/proof_field_presets.json`.

Generated public SVGs must contain:

- an accessible title and description;
- no machine-local path;
- no secret;
- no fabricated benchmark or performance result;
- no unsupported command shape.

## Review checklist

- Is at least 75% of the composition neutral or near-black?
- Is color concentrated in meaningful graph paths?
- Can a reader distinguish proof, candidate, and unknown without relying on hue alone?
- Are labels real product vocabulary or clearly illustrative?
- Is the headline readable before the artwork?
- Does the mobile composition preserve the verification idea rather than merely crop the desktop image?
- Does reduced-motion mode remain complete?
- Does the copy stay within `docs/design/CLAIMS_AND_COPY.md`?
