# Buildroot Text Evidence Mini Fixture

This fixture is a deterministic mini Buildroot-style corpus for Stage 0 text evidence.

It includes Buildroot planning surfaces that should become text evidence before broad vector or parser work:

- `package/foo/foo.mk`
- `package/foo/Config.in`
- `docs/manual/adding-packages.adoc`
- `support/scripts/pkg-stats`

It also includes `src/download.c` so a parser-supported C file remains graph-parsed while the Buildroot planning files remain text evidence, not graph proof.
