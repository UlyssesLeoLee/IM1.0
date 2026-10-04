# syntax=docker/dockerfile:1.7
# im-migrate Dockerfile —— 装**仓内自带的** im-migrate 二进制
# 依据: ImplementationSpec §8.2 P0-4 修复
# 用于 K3s Job 跑 DB 迁移

# ## 2026-10-04 重写: 此前这个镜像装的是 sqlx CLI, 与 migrate-job.yaml 直接矛盾
#
# `deploy/k3s/dev/migrate-job.yaml` 从 2026-10-03 起就写着:
#   command: ["/app/im-migrate"]
# 并附注释「不用 `command: ["sqlx","migrate","run"]`: 那要求镜像里装 sqlx CLI,
# 于是 SQL 与 CLI 都得挂进容器, 版本对应关系就断了」。
#
# **但这个 Dockerfile 当时没跟着改** —— 它 `cargo install sqlx-cli`、
# ENTRYPOINT 指向 sqlx、把 `migrations/` 当**独立目录** COPY 进去。
# 两者对不上的后果很具体: Job 跑 `/app/im-migrate`, 而镜像里**根本没有这个文件**
# -> 容器直接 crash, 迁移永远跑不了。
#
# 之所以没人发现: **镜像从来没被真正构建过**(见 gap-ledger §1.25)。

FROM rust:1-slim AS builder
WORKDIR /build

# 不需要 protoc / libprotobuf-dev: im-migrate 只依赖
# im-common / sqlx / tokio / tracing / tracing-subscriber, **不经过 im-proto**
# (im-proto 的 build.rs 才是要 well-known types 的那个)。
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
# `migrations/` 必须 COPY 进**构建期**: `sqlx::migrate!("../../migrations")`
# 在编译期就把 7 份 SQL 内嵌进二进制, 路径相对 CARGO_MANIFEST_DIR
# (即 crates/im-migrate/), 少 COPY 会在编译期就报找不到目录。
# 内嵌之后**运行期**就不再需要这个目录 —— 镜像里只有二进制一个文件,
# 它携带的迁移与编译它的代码必然同版本。
COPY migrations ./migrations
COPY crates ./crates
RUN mkdir -p target && \
    cargo build --release -p im-migrate --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -M -s /usr/sbin/nologin im

# 路径与 migrate-job.yaml 的 `command: ["/app/im-migrate"]` **必须一致**。
COPY --from=builder /build/target/release/im-migrate /app/im-migrate

USER 1000:1000
ENTRYPOINT ["/app/im-migrate"]
