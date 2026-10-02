# syntax=docker/dockerfile:1.7
# im-migrate Dockerfile — 只含 sqlx-cli(ImplementationSpec §8.2 P0-4 修复)
# 用于 K3s Job 跑 DB 迁移

FROM rust:1-slim AS builder
WORKDIR /build
# 2026-10-03 修: 原为 '^0.8',是 sqlx 0.8.6 -> 0.9.0 升级的漏网之鱼 ——
# lane 2 只把 .github/workflows/ci.yml 对齐成 ^0.9, 这里没跟着改, 导致
# CI 用 sqlx 0.9 验证的 migration, 到 K3s 上却由 sqlx 0.8 执行, 两边行为不一致。
# 必须与 ci.yml "install sqlx-cli" 和 workspace 的 sqlx = "0.9" 三处对齐。
RUN cargo install sqlx-cli --version '^0.9' --no-default-features --features rustls,postgres --locked

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
