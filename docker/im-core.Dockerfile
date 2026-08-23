# syntax=docker/dockerfile:1.7
# im-core Dockerfile
# 依据: ImplementationSpec §8.1

FROM rust:1-slim AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN mkdir -p target && \
    cargo build --release -p im-core --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -M -s /usr/sbin/nologin im

# MVP: im-core 与 im-gateway 共享 im-gateway 二进制(内部嵌入 im-core)
# V1 拆分为独立进程时,这里改用专门的 im-core 二进制
COPY --from=builder /build/target/release/im-core /usr/local/bin/im-core 2>/dev/null || \
     COPY --from=builder /build/target/release/im-gateway /usr/local/bin/im-core

USER 1000:1000
EXPOSE 8080 9000
ENTRYPOINT ["/usr/local/bin/im-core"]
