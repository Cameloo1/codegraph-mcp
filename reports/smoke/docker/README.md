# Docker Smoke Reports

`scripts/smoke_docker.sh` writes Docker build and runtime smoke logs here.

The Dockerfile builds the Rust workspace, runs the deterministic fixture index
during image build, and defaults to `codegraph-mcp --help` at runtime.
