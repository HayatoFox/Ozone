# Image de l'instance Ozone (mode tout-en-un SQLite). Runtime Debian — tourne sur
# n'importe quel hôte Docker, y compris AlmaLinux/RHEL. Pour une image base AlmaLinux,
# voir deploy/Dockerfile.almalinux.
# Build multi-étapes : compilation Rust → image runtime minimale.

# Image `rust` complète (inclut gcc/build-essential nécessaires au SQLite embarqué).
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p ozone-api

FROM debian:bookworm-slim
RUN useradd -m -u 10001 ozone && mkdir -p /data && chown ozone:ozone /data
COPY --from=builder /app/target/release/ozone-api /usr/local/bin/ozone-api
USER ozone
ENV OZONE_BIND=0.0.0.0:8080 \
    OZONE_DB_PATH=/data/ozone.db \
    RUST_LOG=info
VOLUME ["/data"]
EXPOSE 8080
CMD ["ozone-api"]
