# syntax=docker/dockerfile:1.7
# im-core Dockerfile
# 依据: ImplementationSpec §8.1
#
# ## ⚠ 这个镜像里**没有 im-core 二进制** —— 它跑的是 im-gateway
#
# 事实(2026-10-04 首次真构建时查实): `crates/im-core` **没有 src/main.rs**,
# 是纯库 crate(Cargo.toml 里也没有 [[bin]])。所以
#   `cargo build --release -p im-core`
# **不会产出任何可执行文件**。
#
# 原先这里写的是:
#   COPY --from=builder /build/target/release/im-core /usr/local/bin/im-core 2>/dev/null || \
#        COPY --from=builder /build/target/release/im-gateway /usr/local/bin/im-core
# 三个问题叠在一起:
#   1. `COPY` 后面接 `2>/dev/null || ...` —— **Dockerfile 里没有 shell**,
#      那串会被当成源文件路径的一部分。这行语法非法。
#   2. 源文件 `/build/target/release/im-core` **永远不存在**(见上)。
#   3. 就算能跑, 一个「名字叫 im-core、内容是 im-gateway」的二进制, 会让
#      排障的人对着 gateway 的日志找 core 的问题 —— **主动误导**。
#
# 故此处显式构建 im-gateway 并**如实命名**。im-core 作为库已被 im-gateway
# 进程内链接(im-gateway 直接依赖 im-core), 不需要独立进程。
# **V1 若真拆成独立进程, 应在 im-core 有了真实 main.rs 之后再改这里** ——
# 那时才有真正的二进制可 COPY, 现在写只会是在给一个不存在的东西占位。

FROM rust:1-slim AS builder
WORKDIR /build

# `libprotobuf-dev` 不是冗余: core.proto import 了 google/protobuf/{empty,timestamp}.proto,
# 而 build.rs 没设 PROTOC_INCLUDE, 完全依赖系统那份 well-known types。
# Ubuntu 的 protobuf-compiler 顺带提供, Debian 的不带 —— 少它 CI 绿而 Docker 红。
# 详见 docker/im-gateway.Dockerfile 的同名校验说明。
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates protobuf-compiler libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN mkdir -p target && \
    cargo build --release -p im-gateway --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -M -s /usr/sbin/nologin im

# 如实命名: 镜像是 im-core 管线的一部分, 但它装的和跑的**确实是 im-gateway**。
# 想让人一眼看出来, 就别把它改名成 im-core。
COPY --from=builder /build/target/release/im-gateway /usr/local/bin/im-gateway

USER 1000:1000
# 此前是 `EXPOSE 8080 9000`。9000 是 gRPC 端口, 而本仓**没有任何 gRPC server**
# (`grep -rn 9000 crates/` 无监听方; im-gateway.yaml 的 9000 Service 端口已
# 于 2026-10-03 删除, 见 gap-ledger §1.x)。留着一个永不监听的端口, 只会让
# 使用者以为有可连的 gRPC 端点。
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/im-gateway"]
