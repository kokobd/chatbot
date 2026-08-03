FROM rust:1.88-bookworm AS build

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev \
  && rustup target add wasm32-unknown-unknown \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY native ./native

RUN cargo build --release --locked --bin chatbot-web

FROM debian:bookworm-slim AS runtime

ENV RUST_LOG=info
ENV PORT=8080

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 \
  && rm -rf /var/lib/apt/lists/*

COPY --from=build /app/target/release/chatbot-web /usr/local/bin/chatbot-web
COPY --from=build /app/crates/chatbot-web/assets /app/crates/chatbot-web/assets

EXPOSE 8080

CMD ["/usr/local/bin/chatbot-web"]
