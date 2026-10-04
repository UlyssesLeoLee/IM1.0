# syntax=docker/dockerfile:1.7
# im-gateway Dockerfile
# 依据: ImplementationSpec §8.1

FROM rust:1-slim AS builder
WORKDIR /build

# 安装构建依赖
#
# `libprotobuf-dev` **不是**冗余, 少它这个镜像就构建不出来 —— 这正是
# 「镜像实际构建从未验证」一直遮着的问题(2026-10-04 首次真构建时暴露)。
#
# `crates/im-proto/proto/core.proto` import 了 well-known types:
#   google/protobuf/empty.proto
#   google/protobuf/timestamp.proto
# 而 build.rs 只给了 `&["proto"]` 作 include 路径, **没有设 `PROTOC_INCLUDE`**,
# 所以完全依赖系统那份 well-known types。
#
# 关键在于**两个发行版的打包不同**:
#   - Ubuntu 的 `protobuf-compiler` **顺带**提供 /usr/include/google/protobuf/*.proto
#     -> CI(ubuntu-latest)装完就能编, 一直是绿的
#   - Debian 的 `protobuf-compiler` **只**给 /usr/bin/protoc, 那批 .proto 在
#     `libprotobuf-dev` 里 -> 本镜像(基础镜像是 debian:bookworm-slim)直接
#     protoc 报 "google/protobuf/empty.proto: File not found"
#
# 即: 同一个 build.rs, CI 绿而 Docker 红。写清楚原因, 别当成多余包删掉。
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates protobuf-compiler libprotobuf-dev \
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
# 此前是 `EXPOSE 8080 9000`。9000 是 gRPC 端口, 而本仓**没有任何 gRPC server**:
# `grep -rn 9000 crates/` 无监听方, proto 里的 CoreService 由 im-gateway
# **进程内直接调 im-core**, 不走网络。im-gateway.yaml 的 9000 Service 端口也
# 已于 2026-10-03 删除。留一个永不监听的端口, 只会让使用者以为有可连的 gRPC 端点。
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/im-gateway"]
