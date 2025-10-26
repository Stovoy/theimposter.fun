FROM rust:1.82 as builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim as runtime

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 10001 app

COPY --from=builder /app/target/release/theimposter-backend /usr/local/bin/theimposter-backend

EXPOSE 8080
ENV RUST_LOG=info

USER app
ENTRYPOINT ["/usr/local/bin/theimposter-backend"]
