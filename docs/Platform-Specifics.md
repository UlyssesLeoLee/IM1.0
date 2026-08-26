# Platform Specifics (GitHub + K3s)

> **适用**:IM1.0 项目,基于 `Project-Status.md` §1.3 决议
> **决策**:GitHub 仓库 + GitHub Actions + GHCR + K3s 部署
> **技术栈**:Rust 最新 stable(2026-08)+ actix-web 4.x + PostgreSQL 最新(2026-08)

---

## 1. 仓库结构

### 1.1 仓库位置

```
https://github.com/{org}/im1.0
```

> ⚠️ **待填**:组织名

### 1.2 目录布局

```
im1.0/
├── .github/
│   ├── workflows/
│   │   ├── ci.yml              # 主 CI(PR 触发)
│   │   ├── release.yml         # Release 构建 + 镜像推送
│   │   └── deploy-dev.yml      # 部署到 K3s dev
│   ├── ISSUE_TEMPLATE/
│   │   ├── bug.md
│   │   ├── feature.md
│   │   └── question.md
│   ├── PULL_REQUEST_TEMPLATE.md
│   ├── CODEOWNERS              # 自动指派 reviewer
│   ├── dependabot.yml          # 依赖自动更新
│   └── SECURITY.md             # 漏洞披露政策
├── crates/
│   ├── im-common/              # 错误码 / 共享类型
│   ├── im-protocol/            # WS 消息 / proto
│   ├── im-store/               # DB 访问
│   └── im-gateway/             # HTTP + WebSocket
├── frontend/                   # Next.js(MVP 之后)
├── docs/                       # 项目文档
│   ├── Project-Status.md
│   ├── Workflow.md
│   ├── Workflow-RACI.md
│   ├── Day1-Task-List.md
│   ├── Platform-Specifics.md   # 本文档
│   ├── SRS.md
│   ├── BasicDesign.md
│   ├── DetailedDesign.md
│   ├── SDK-Integration-Guide.md
│   ├── LiveKit-Voice-Subsystem.md
│   ├── Deployment-Runbook.md
│   └── templates/
├── k8s/                        # K3s manifests
│   ├── dev/
│   ├── staging/                # 未来
│   └── prod/                   # 未来
├── scripts/
│   ├── gen_workflow_templates.py
│   └── gen_dd_aux.py
├── .gitignore
├── .gitattributes
├── Cargo.toml                  # workspace
├── Cargo.lock
├── LICENSE
└── README.md
```

### 1.3 .gitignore(最小集)

```gitignore
# Rust
target/
**/*.rs.bk
Cargo.lock.bak

# Node
node_modules/
.next/
.pnpm-store/

# Env
.env
.env.*
!.env.example

# IDE
.vscode/
.idea/
*.swp

# OS
.DS_Store
Thumbs.db

# Build
*.log
*.tmp
```

---

## 2. GitHub 仓库设置

### 2.1 分支保护(main)

设置:
- ✅ Require pull request before merging
- ✅ Require approvals: **1**(极简流程,1 个 approve)
- ✅ Dismiss stale pull request approvals when new commits are pushed
- ✅ Require status checks to pass before merging
  - `lint`
  - `test-unit`
  - `test-integration`
  - `sast`
- ✅ Require conversation resolution before merging
- ❌ Require linear history(允许 merge commit)
- ❌ Require signed commits(内部仓库,不需要)
- ✅ Include administrators(管理员也要走 PR)
- ✅ Allow force pushes: **只允许**给 `main` 的特定维护者(默认不允许)

### 2.2 CODEOWNERS

```
# 默认:所有文件需要 TL 批准
*       @tech-lead

# 文档可由 PM 批准
/docs/  @pm @tech-lead

# Workflow 文档要 Mavis 跳过(自动生成)
/docs/templates/ @tech-lead

# 部署 manifest 需要 SRE 同意
/k8s/   @sre @tech-lead

# 协议 / API 改动需要 2 人
/crates/im-protocol/ @tech-lead @dev-b
/crates/im-gateway/src/api/ @tech-lead @dev-b
```

### 2.3 Secret 配置

在 GitHub Settings → Secrets and variables → Actions:

| Secret | 用途 | 来源 |
|---|---|---|
| `CARGO_REGISTRY_TOKEN` | crates.io 私有发布(暂不用) | — |
| `GHCR_TOKEN` | 推送到 GHCR | 自动(actions) |
| `DATABASE_URL_DEV` | dev K3s Postgres 连接 | k8s secret |
| `JWT_SECRET_DEV` | JWT 签名密钥(64 字节) | openssl rand |
| `KUBECONFIG_DEV` | K3s dev namespace kubeconfig | base64 |
| `SLACK_WEBHOOK` | CI 通知 | workspace |

### 2.4 Dependabot

```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 10
    groups:
      minor-and-patch:
        update-types: ["minor", "patch"]
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
```

### 2.5 漏洞披露

`.github/SECURITY.md`:
- 报告邮箱:`security@example.com`
- 响应 SLA:Critical 24h,High 7d,Medium 30d
- 不公开漏洞细节直到补丁发布

---

## 3. CI / CD(GitHub Actions)

### 3.1 ci.yml(主流水线,PR 触发)

```yaml
name: CI

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1
  RUSTFLAGS: "-D warnings"

jobs:
  lint:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test-unit:
    name: Unit Tests
    runs-on: ubuntu-latest
    needs: lint
    services:
      postgres:
        image: postgres:18.6
        env:
          POSTGRES_USER: im
          POSTGRES_PASSWORD: im
          POSTGRES_DB: im_test
        ports: [5432:5432]
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --all-features
        env:
          DATABASE_URL: postgres://im:im@localhost:5432/im_test

  test-integration:
    name: Integration Tests
    runs-on: ubuntu-latest
    needs: test-unit
    services:
      postgres:
        image: postgres:18.6
        env:
          POSTGRES_USER: im
          POSTGRES_PASSWORD: im
          POSTGRES_DB: im_test
        ports: [5432:5432]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --test '*' -- --test-threads=1
        env:
          DATABASE_URL: postgres://im:im@localhost:5432/im_test

  sast:
    name: Static Analysis
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: cargo audit
        run: |
          cargo install --locked cargo-audit
          cargo audit
      - name: semgrep
        uses: returntocorp/semgrep-action@v1
        with:
          config: >-
            p/rust
            p/security-audit
            p/secrets
            p/owasp-top-ten
```

### 3.2 release.yml(tag 触发,推 GHCR)

```yaml
name: Release

on:
  push:
    tags: ['v*.*.*']

jobs:
  build-and-push:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    strategy:
      matrix:
        crate: [im-gateway, im-store]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - name: Build & push
        uses: docker/build-push-action@v5
        with:
          context: .
          file: ./docker/${{ matrix.crate }}.Dockerfile
          push: true
          tags: |
            ghcr.io/${{ github.repository_owner }}/im1.0-${{ matrix.crate }}:${{ github.ref_name }}
            ghcr.io/${{ github.repository_owner }}/im1.0-${{ matrix.crate }}:latest
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

### 3.3 deploy-dev.yml(merge to main,推 K3s dev)

```yaml
name: Deploy Dev

on:
  push:
    branches: [main]
  workflow_dispatch:

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: dev
    steps:
      - uses: actions/checkout@v4
      - name: Setup kubeconfig
        env:
          KUBECONFIG_SECRET: ${{ secrets.KUBECONFIG_DEV }}
        run: |
          mkdir -p ~/.kube
          echo "$KUBECONFIG_SECRET" | base64 -d > ~/.kube/config
      - name: Deploy
        run: |
          kubectl apply -f k8s/dev/ -n im1-dev
          kubectl rollout status deployment/im-gateway -n im1-dev --timeout=2m
      - name: Smoke test
        run: ./scripts/smoke.sh https://im-dev.example.com
        env:
          IM_BASE_URL: https://im-dev.example.com
```

---

## 4. K3s 部署

### 4.1 本地开发(k3d 单节点)

```bash
# 安装 k3d
curl -s https://raw.githubusercontent.com/k3d-io/k3d/main/install.sh | bash

# 创建集群
k3d cluster create im1-dev --port 8080:80@loadbalancer

# 安装 ingress
kubectl apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/main/deploy/static/provider/k3s/deploy.yaml

# 部署
kubectl apply -f k8s/dev/ -n im1-dev
```

### 4.2 命名空间布局

```
im1-dev       # 开发(staging)
im1-staging   # 预生产(未来)
im1-prod      # 生产(未来)
```

### 4.3 最小 manifests(`k8s/dev/`)

`namespace.yaml`:
```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: im1-dev
```

`postgres.yaml`:
```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: postgres-data
  namespace: im1-dev
spec:
  accessModes: [ReadWriteOnce]
  resources:
    requests:
      storage: 5Gi
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: postgres
  namespace: im1-dev
spec:
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
        - name: postgres
          image: postgres:18.6.6
          env:
            - name: POSTGRES_USER
              value: im
            - name: POSTGRES_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: postgres-secret
                  key: password
            - name: POSTGRES_DB
              value: im
          ports:
            - containerPort: 5432
          volumeMounts:
            - name: data
              mountPath: /var/lib/postgresql/data
      volumes:
        - name: data
          persistentVolumeClaim:
            claimName: postgres-data
```

`im-gateway.yaml`:
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: im-gateway
  namespace: im1-dev
spec:
  replicas: 1
  selector:
    matchLabels:
      app: im-gateway
  template:
    metadata:
      labels:
        app: im-gateway
    spec:
      containers:
        - name: im-gateway
          image: ghcr.io/yourorg/im1.0-im-gateway:latest
          ports:
            - containerPort: 8080
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: im-gateway-secret
                  key: database-url
            - name: JWT_SECRET
              valueFrom:
                secretKeyRef:
                  name: im-gateway-secret
                  key: jwt-secret
          readinessProbe:
            httpGet:
              path: /readyz
              port: 8080
            initialDelaySeconds: 5
          livenessProbe:
            httpGet:
              path: /healthz
              port: 8080
            initialDelaySeconds: 30
---
apiVersion: v1
kind: Service
metadata:
  name: im-gateway
  namespace: im1-dev
spec:
  selector:
    app: im-gateway
  ports:
    - port: 80
      targetPort: 8080
  type: ClusterIP
```

`ingress.yaml`:
```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: im-gateway
  namespace: im1-dev
  annotations:
    nginx.ingress.kubernetes.io/proxy-read-timeout: "3600"
    nginx.ingress.kubernetes.io/proxy-send-timeout: "3600"
spec:
  rules:
    - host: im-dev.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: im-gateway
                port:
                  number: 80
```

---

## 5. 技术栈选型(Rust + Web 框架 + DB)

> **决议**(Day 1 Kickoff):用最新 Rust + 最新 PostgreSQL + actix-web 4.x。

### 5.1 Rust

- **版本**:最新 stable(2026-08)
- **更新策略**:用 `rust:1` major tag 自动跟随新 stable(避免锁死老版本,享受 6 周一个 minor)
- **本地安装**:`rustup toolchain install stable && rustup default stable`
- **CI 工具链**:`dtolnay/rust-toolchain@stable`(已在 §3.1 用,跟随 GitHub Action 仓库的 latest stable)
- **Lockfile**:`Cargo.lock` 必须入库(保证可复现构建)

### 5.2 Web 框架 = actix-web 4.x

- **选型理由**:
  - 生产级稳定性(actix 在 TechEmpower benchmark 长期 Top)
  - 完整 HTTP / WebSocket / 中间件生态
  - 与 tokio 集成良好
  - 中文社区活跃
- **Cargo.toml 依赖**:
  ```toml
  [dependencies]
  actix-web = "4"          # 4.x 最新
  actix-ws = "0.3"         # WebSocket 支持
  actix-rt = "2"
  ```
- **辅助 crate**:
  - `actix-web-httpauth` — JWT / Bearer 中间件
  - `actix-cors` — CORS
  - `tracing` / `tracing-actix-web` — 日志与链路追踪
- **WebSocket**:用 `actix-ws` 0.3(MVP);后续可换 `actix-web-actors`
- **不用 axum** / tower / hyper:统一技术栈,降低 2-3 人极简团队的学习成本

### 5.3 PostgreSQL

- **版本**:18.6(2026-08 锁 patch level,2026-08-26 per Ulysses 指令)
- **CI / 本地**:`postgres:18.6` Docker image
- **驱动 crate**:`sqlx = { version = "0.9", features = ["runtime-tokio", "postgres", "macros", "migrate"] }`(2026-08-26 已升 0.9 per Day 1 GATE 补签)
- **迁移工具**:`sqlx migrate`(无需独立 migration 工具)
- **生产前升级检查**:PG 18.6 → 18.7 / 19.x 时,先在 staging 跑一周

### 5.4 不用 / 暂不引入

- ❌ axum(统一用 actix-web)
- ❌ tower(actix 已有中间件)
- ❌ hyper(actix 内部用,无需直接依赖)
- ❌ diesel(用 sqlx,async 友好)
- ❌ Redis(MVP 阶段不需要)
- ❌ LiveKit(留给 Voice 阶段)

---

## 6. 镜像构建(Dockerfile 示例)

`docker/im-gateway.Dockerfile`:
```dockerfile
# 阶段 1: 构建
FROM rust:1-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 缓存依赖
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN mkdir -p target && cargo build --release -p im-gateway

# 阶段 2: 运行时
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/im-gateway /usr/local/bin/im-gateway

USER 1000:1000
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/im-gateway"]
```

---

## 7. 监控 / 日志(最小集,MVP 阶段不引入)

| 项 | MVP 阶段 | 之后 |
|---|---|---|
| 监控 | K8s 自带(无 metrics) | Prometheus + Grafana |
| 日志 | kubectl logs | Loki + Promtail |
| Trace | 无 | Jaeger / OpenTelemetry |
| 告警 | 无 | Alertmanager + Slack |

> 详见 `Deployment-Runbook.md`(已有)+ 未来补的 SRE 文档。

---

## 8. 模板填写指南(基于本平台)

下列模板在 `docs/templates/` 中,本节给出本平台特定的具体填充建议。

### 8.1 53 開發環境

- 操作系统:Ubuntu 22.04 / macOS 14 / WSL2
- 必备工具:
  - **Rust 最新 stable**(`rustup toolchain install stable && rustup default stable`)
  - **PostgreSQL 客户端**(`psql` 16+)
  - Node 20+(前端)
  - Docker 24+ / k3d v5+
  - `cargo install cargo-audit cargo-watch sqlx-cli`
- IDE:VSCode + `rust-analyzer` + `Even Better TOML`
- 推荐插件:`actix-web` 路由跳转、`sqlx` SQL 校验

### 8.2 55 静态解析 / 58 CI Pipeline

直接用 §3.1 的 `ci.yml` 内容(已含 lint / unit / integration / sast 4 个 stage)。

### 8.3 104 本番環境

- 镜像:GHCR(`ghcr.io/{org}/im1.0-im-gateway` + `im1.0-im-store`)
- Rust 镜像:`rust:1-slim`(详见 §5)
- IaC:暂时手写 YAML,后续上 Helm 或 Kustomize
- 部署触发:push to main → deploy-dev job → K3s dev namespace
- 生产部署:本阶段无,详见 `Deployment-Runbook.md`

### 8.4 110 监控

MVP 阶段:仅 K8s probe(`/healthz` `/readyz`)+ 日志
后续:`Platform-Specifics.md` §6 表格

---

## 9. 维护

- 本文档随平台变更更新(版本变化 / 工具替换)
- 新增 GitHub Actions 必同步更新本文件 §3
- 任何 K3s 部署变更必更新 §4

---

**维护**:SRE | **版本**:v1.0.0 | **最后更新**:2026-08-20
