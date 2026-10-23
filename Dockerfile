# syntax=docker/dockerfile:1

# --- web client ---------------------------------------------------------------
FROM node:22-alpine AS web
WORKDIR /web
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# --- server -------------------------------------------------------------------
FROM rust:1-bookworm AS build
WORKDIR /src

# Build the dependencies first, so editing the sources does not rebuild them.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && touch src/lib.rs \
    && cargo build --release --bin dragon-city \
    && rm -rf src

COPY migrations ./migrations
COPY src ./src
# Cargo compares file times: make sure the real sources count as newer than the placeholders.
RUN touch src/main.rs src/lib.rs && cargo build --release --bin dragon-city

# --- runtime ------------------------------------------------------------------
# No shell, no package manager: just the binary and the game data it needs.
FROM gcr.io/distroless/cc-debian12:nonroot AS app

WORKDIR /app
COPY --from=build /src/target/release/dragon-city /app/dragon-city

# Game data: the config/template/PvP seeds are small and baked in; the Flash files, the
# assets (hundreds of MB) and the mapped domains are mounted by compose.yaml.
COPY web/srv /app/seed
COPY --from=web /web/dist /app/frontend/dist

ENV SEED_DIR=/app/seed \
    FRONTEND_DIST=/app/frontend/dist \
    FLASH_DIR=/data/flash \
    ASSETS_DIR=/data/assets \
    DOMAIN_MAP_DIR=/data/domains \
    BIND_ADDR=0.0.0.0:8080 \
    TRUST_PROXY=false

EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=40s --retries=3 \
    CMD ["/app/dragon-city", "healthcheck"]

# `serve` waits for the databases and applies the migrations before listening.
ENTRYPOINT ["/app/dragon-city"]
CMD ["serve"]
