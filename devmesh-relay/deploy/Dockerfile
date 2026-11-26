FROM node:24-bookworm-slim AS web
WORKDIR /src
COPY package.json package-lock.json ./
COPY devmesh-protocol/package.json devmesh-protocol/package.json
COPY devmesh-agent/packages/pi-extension/package.json devmesh-agent/packages/pi-extension/package.json
COPY devmesh-agent/packages/pi-durable-host/package.json devmesh-agent/packages/pi-durable-host/package.json
COPY devmesh-relay/web/package.json devmesh-relay/web/package.json
RUN npm ci --ignore-scripts --no-audit --no-fund
COPY devmesh-protocol/ts devmesh-protocol/ts
COPY devmesh-relay/web devmesh-relay/web
RUN npm run build --workspace @devmesh/web
FROM rust:1.89-bookworm AS rust
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY devmesh-protocol devmesh-protocol
COPY devmesh-agent devmesh-agent
COPY devmesh-relay devmesh-relay
RUN cargo build --locked --release -p devmesh-relay
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates postgresql-client && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust /src/target/release/devmesh-relay /usr/local/bin/devmesh-relay
COPY --from=web /src/devmesh-relay/web/dist /app/web/dist
EXPOSE 17843
ENTRYPOINT ["devmesh-relay"]
CMD ["serve", "--listen", "0.0.0.0:17843", "--web-dir", "/app/web/dist"]
