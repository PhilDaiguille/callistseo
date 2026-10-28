# syntax=docker/dockerfile:1
# Image unique : le serveur CallistSEO (API + interface web). Contexte de build : la racine du dépôt.

# --- 1. Interface web -------------------------------------------------------------------------
FROM node:26-slim AS web
RUN npm install -g pnpm@12
WORKDIR /src
COPY package.json pnpm-workspace.yaml pnpm-lock.yaml ./
COPY apps/web/package.json apps/web/
COPY apps/desktop/package.json apps/desktop/
RUN pnpm install --frozen-lockfile --filter web...
COPY apps/web apps/web
RUN pnpm --filter web build

# --- 2. Serveur Rust ----------------------------------------------------------------------------
FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
# L'application desktop est un membre du workspace : son manifeste doit exister, elle n'est pas compilée.
COPY apps/desktop/src-tauri apps/desktop/src-tauri
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p callistseo-cli \
    && cp target/release/callistseo /callistseo

# --- 3. Image finale ----------------------------------------------------------------------------
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin callistseo \
    && mkdir -p /data && chown callistseo /data
COPY --from=server /callistseo /usr/local/bin/callistseo
COPY --from=web /src/apps/web/dist /app/web
USER callistseo
VOLUME /data
EXPOSE 8080
# Sonde sans dépendance supplémentaire : bash et /dev/tcp.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD bash -c 'exec 3<>/dev/tcp/127.0.0.1/8080 && printf "GET /api/health HTTP/1.0\r\n\r\n" >&3 && head -n1 <&3 | grep -q " 200 "'
ENTRYPOINT ["callistseo"]
# Le jeton vient de CALLISTSEO_TOKEN ; sans lui, le serveur refuse de démarrer hors loopback.
CMD ["serve", "--db", "/data/audits.db", "--addr", "0.0.0.0:8080", "--static-dir", "/app/web"]
