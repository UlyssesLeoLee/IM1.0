# syntax=docker/dockerfile:1.7
# im-migrate Dockerfile — 只含 sqlx-cli(ImplementationSpec §8.2 P0-4 修复)
# 用于 K3s Job 跑 DB 迁移

FROM rust:1-slim AS builder
WORKDIR /build
RUN cargo install sqlx-cli --version '^0.8' --no-default-features --features rustls,postgres --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -M -s /usr/sbin/nologin im

COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx

# 迁移文件随镜像一起 COPY
COPY migrations /migrations

USER 1000:1000
WORKDIR /migrations
ENTRYPOINT ["sqlx"]
CMD ["migrate", "run"]
