FROM rust:1.88-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release --locked --bin enhanced-gmail-mcp

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 10001 app && mkdir -p /data /attachments && chown -R app:app /data /attachments
COPY --from=builder /build/target/release/enhanced-gmail-mcp /usr/local/bin/enhanced-gmail-mcp
USER app
EXPOSE 3000
ENTRYPOINT ["enhanced-gmail-mcp"]
