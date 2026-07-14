# CodeGraph brand asset generation

`generate_proof_field.py` produces the promoted SVG assets used by the root README.

Run from the repository root:

```bash
python scripts/brand/generate_proof_field.py --all
```

The generator is deterministic and uses only the Python standard library. Presets live in `proof_field_presets.json`.

Generated assets:

- `docs/assets/readme/title-pic.svg`
- `docs/assets/readme/agent_use_loop.svg`
- `docs/assets/readme/proof_taxonomy.svg`
- `docs/assets/readme/codegraph_terminal.svg`

The assets use real CodeGraph terminology but do not read local graph DBs, raw reports, benchmark output, or machine-specific paths. Review the generated SVG text before promotion and run:

```bash
python scripts/check_readme_artifacts.py
python scripts/check_markdown_links.py
python scripts/check_docs_hygiene.py
```
