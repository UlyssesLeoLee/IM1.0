# syntax=docker/dockerfile:1.7
# im-gateway Dockerfile
# 依据: ImplementationSpec §8.1

FROM rust:1-slim AS builder
WORKDIR /build

# 安装构建依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# 缓存依赖层(只 COPY Cargo.toml/lock,避免代码改动触发全量重编)
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN mkdir -p target && \
    cargo build --release -p im-gateway --locked

# 运行时镜像
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -M -s /usr/sbin/nologin im

COPY --from=builder /build/target/release/im-gateway /usr/local/bin/im-gateway

USER 1000:1000
EXPOSE 8080 9000
ENTRYPOINT ["/usr/local/bin/im-gateway"]
