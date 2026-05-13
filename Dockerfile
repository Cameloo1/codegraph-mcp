FROM rust:1-bookworm

WORKDIR /app

COPY . .

RUN cargo build --workspace --locked
RUN ./target/debug/codegraph-mcp index fixtures/smoke/basic_repo --db /tmp/codegraph-basic-repo.sqlite --profile --json

CMD ["./target/debug/codegraph-mcp", "--help"]
