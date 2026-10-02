# im-gateway 生产镜像 —— 2026-10-03 新增
#
# ## 刻意**不写** `# syntax=docker/dockerfile:1`
#
# 那行会让 BuildKit 先去 registry 拉 `docker/dockerfile:1`, 于是**在解析本文件
# 之前**就依赖外网。本仓库用到的指令(multi-stage / COPY / RUN / USER / ENTRYPOINT)
# 内置 frontend 全都支持, 不需要新 frontend 特性。
# 在受限网络(本机开发环境即如此: registry 与 GitHub 一样被代理掐断)下, 少一次
# 拉取就少一个「明明 Dockerfile 没问题却先失败」的原因。
# 若将来用了 heredoc(`<<EOF`)等较新指令, 再把那行加回来。
#
# ## 为什么此前没有这个文件
#
# `deploy/k3s/dev/im-gateway.yaml` 引用 `ghcr.io/yourorg/im1.0-im-gateway:latest`,
# 但**仓库里没有任何东西能构建它** —— 没有 Dockerfile, 没有 CI build job。
# 也就是说 F-2 (K3s 部署) 在补上这个文件之前是无法开始的: 清单引用的镜像
# 只能靠人手写, 写错没有任何检查能发现。
#
# 本文件是「让清单第一次真的能被部署」的第一步。
#
# ## 关键决策
#
# - **多阶段构建**: 编译需要 toolchain + 依赖源码, 运行只需要一个静态链接的
#   二进制 + CA 证书。不把 builder 阶段的 ~1.5GB 工具链带进最终镜像。
# - **toolchain 显式钉住**: 仓库**没有** `rust-toolchain.toml`, 而本机在
#   1.98.1 上开发。若不钉住, 镜像会用 rust:latest 构建, 本地能过的代码到镜像里
#   可能因为新 lint 或 MSRV 变化而失败 —— 且这种失败只在构建镜像时出现。
# - **`--locked`**: 用提交的 `Cargo.lock` 构建。若 lock 与 Cargo.toml 不一致,
#   构建必须失败而不是静默更新 lock(否则镜像里的依赖版本与代码评审时看到的不同)。
# - **非 root 运行**: 商业产品的最低基线。
# - **不 COPY migrations/**: 迁移由独立的 migrate job 负责(职责分离, 且
#   迁移权限不该给长期运行的服务)。见 `deploy/k3s/dev/migrate-job.yaml`。

# ============================================================================
# Stage 1 — builder
# ============================================================================
FROM rust:1.98.1-slim-bookworm AS builder

# pkg-config + libssl-dev: 依赖树里 openssl-sys / ring 等需要。
#   --no-install-recommends + rm -rf /var/lib/apt/lists/* : 避免把 apt 索引
#   留在层里(镜像会平白大几十 MB, 且这些层不可变、永不失效)。
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# 依赖清单先 COPY: 只要 Cargo.toml / Cargo.lock 没变, 这一层就命中缓存,
# 改代码时不必重编全部依赖。
COPY Cargo.toml Cargo.lock ./
COPY crates/im-common/Cargo.toml       crates/im-common/
COPY crates/im-proto/Cargo.toml        crates/im-proto/
COPY crates/im-protocol/Cargo.toml     crates/im-protocol/
COPY crates/im-core/Cargo.toml         crates/im-core/
COPY crates/im-gateway/Cargo.toml      crates/im-gateway/
COPY crates/im-presence/Cargo.toml     crates/im-presence/
COPY crates/im-media/Cargo.toml        crates/im-media/
COPY crates/extension-runtime/Cargo.toml crates/extension-runtime/
COPY crates/im-testkit/Cargo.toml      crates/im-testkit/

# 造一层「只有依赖、没有源码」的假二进制, 让 cargo 先把依赖编好。
# 手法: 用一个最小 main.rs 编出与真 crate 同名的 bin, 之后 COPY 真源码覆盖,
# cargo 只重编 workspace 内的 crate, 依赖层保持命中。
RUN mkdir -p crates/im-gateway/src \
    && echo 'fn main() {}' > crates/im-gateway/src/main.rs \
    && cargo build --release --locked -p im-gateway \
    && rm -rf crates/im-gateway/src

# 现在放真源码。注意 build.rs: im-proto 用 tonic-build 跑 protoc,
# 所以**必须**把 proto 目录一起带进来。
COPY crates/ crates/
RUN cargo build --release --locked -p im-gateway \
    && strip target/release/im-gateway

# ============================================================================
# Stage 2 — runtime
# ============================================================================
FROM debian:bookworm-slim AS runtime

# ca-certificates: 容器要访问 postgres / NATS(将来) / 镜像仓库,
#   缺 CA 证书的表现是 TLS 握手失败, 且错误信息很难指向「缺包」。
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 非 root 运行。uid/gid 固定而非用 adduser: 避免不同基础镜像的默认 uid 不一致,
# 导致挂载 volume 的属主对不上。
RUN groupadd --system --gid 10001 im \
    && useradd --system --uid 10001 --gid im --no-create-home --shell /usr/sbin/nologin im

WORKDIR /app
COPY --from=builder /build/target/release/im-gateway /app/im-gateway

USER 10001:10001

# 8080 与 im-gateway.yaml 的 containerPort 一致(main.rs 读 IM_HTTP_PORT, 默认 8080)
EXPOSE 8080

# 探针用 /healthz 与 /readyz, 与清单里的 readinessProbe/livenessProbe 对应。
# 这里用 exec form 起二进制自己探 —— 镜像里没有 curl/wget, 装了会为了
# 「有个探针」而给运行镜像增加攻击面; 而且 HTTP 探针由 kubelet 从集群侧发起,
# 不需要镜像内有客户端。
ENTRYPOINT ["/app/im-gateway"]
