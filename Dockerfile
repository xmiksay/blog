# -- Build --
FROM rust:1.88-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release --bin blog_server --bin blog_cli --bin blog_migration

# -- Runtime --
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/blog_server ./
COPY --from=builder /app/target/release/blog_cli ./
COPY --from=builder /app/target/release/blog_migration ./
COPY static/ ./static/
COPY templates/ ./templates/
EXPOSE 3000
ENTRYPOINT ["./blog_server"]
