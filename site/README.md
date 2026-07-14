# CodeGraph product site

This directory is a dependency-free static product site for the **Verified Signal Field** visual system.

Run a local preview from the repository root:

```bash
python -m http.server 8000 --directory site
```

Then open `http://127.0.0.1:8000`.

The site is intentionally static and self-contained:

- no CDN assets;
- no analytics or network calls;
- deterministic SVG proof-field generation in `proof-field.js`;
- native Intersection Observer and Web Animations APIs;
- reduced-motion support;
- copy constrained by the public CodeGraph proof and claim boundaries.

The product-frame graph is explicitly illustrative. It uses real CodeGraph relation and exactness vocabulary but is not presented as benchmark or repository evidence. The live local product surface remains `codegraph-mcp serve-ui`.
