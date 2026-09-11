# Day 1 任务清单 (IM Core MVP - 消息收发)

> **目标**:2 周内交付 **消息收发最小闭环**
> - 1 个用户能登录拿 Token
> - 1 个用户能给另 1 个用户发消息
> - 接收方实时收到(WebSocket)
> - 消息持久化,刷新后能查历史
>
> **参考**:`docs/Project-Status.md` §1.1
>
> **WBS 全量索引**:`docs/132-wbs.md` v1.0.0 (Phase A-H, 41 项, token 单位, 完整 predecessor/successor/lag)
> 本清单只覆盖 Day 1-14 MVP (Phase A-F);Phase G V1 移交 / Phase H 决策待办 见 132-wbs.md

---

## 0. 总览

| 阶段 | 天数 | 任务 | 里程碑 |
|---|---|---|---|
| **Phase A:启动** | Day 1-2 | 仓库 / CI / 文档 / 命名 | 仓库可 clone + CI 跑通 |
| **Phase B:数据 + 协议** | Day 3-5 | DB schema + WS 协议 + API spec | 接口冻结 |
| **Phase C:实现** | Day 6-9 | im-gateway 实现 | 单测通过 |
| **Phase D:联调** | Day 10-11 | 端到端 smoke | 能发能收 |
| **Phase E:部署** | Day 12-13 | K3s dev namespace 跑通 | dev 演示 |
| **Phase F:复盘** | Day 14 | 团队复盘 + 下一步 | 复盘报告 |

> 14 天 = 2 个完整工作周 + 1 个缓冲日

---

## Phase A:启动(Day 1-2)

### Day 1 - 仓库 + CI + 文档

**上午(4h)**:
- [ ] 创建 GitHub 仓库 `im1.0`(主仓库,public 或 private 待定)
- [ ] 初始化 `cargo workspace` 结构:
  ```
  im1.0/
  ├── Cargo.toml          # workspace root(actix-web 4.x + sqlx 0.8)
  ├── crates/
  │   ├── im-gateway/     # HTTP + WebSocket(actix-web 4)
  │   ├── im-store/       # DB 访问(sqlx 0.8)
  │   ├── im-common/      # 共享类型 / 错误码
  │   └── im-protocol/    # WS 消息 / 错误码定义
  ├── frontend/           # Next.js(放后)
  ├── docs/               # 已有
  ├── .github/workflows/  # CI 配置
  ├── k8s/                # K3s manifests(postgres:18.6)
  └── README.md
  ```
- [ ] 写 `Cargo.toml` workspace 根(关键依赖锁版本):
  ```toml
  [workspace.dependencies]
  actix-web = "4"
  actix-ws = "0.3"
  actix-rt = "2"
  sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "macros", "migrate"] }
  tokio = { version = "1", features = ["full"] }
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  tracing = "0.1"
  ```
- [ ] 创建 `.github/workflows/ci.yml`(详见 `Platform-Specifics.md` §3)
- [ ] 创建 `.github/ISSUE_TEMPLATE/`(bug / feature / question 模板)
- [ ] 创建 `.github/PULL_REQUEST_TEMPLATE.md`(关联 `aux-10` checklist)

**下午(4h)**:
- [ ] 写 `docs/Project-Status.md` ✓(已完成)
- [ ] 写 `docs/Workflow-RACI.md` ✓(已完成)
- [ ] 写 `docs/Day1-Task-List.md` ✓(本文档)
- [ ] 写 `docs/Platform-Specifics.md`(待完成)
- [ ] **填 aux-01 命名规范**(明确 Rust / TS / DB 命名,所有 PR 都强制)
- [ ] **填 aux-03 错误码注册表**(列出 IM Core 阶段的错误码,见 §6)
- [ ] **填 aux-13 协议帧样例**(auth / publish / receive 完整 JSON)
- [ ] 在 `Cargo.toml` 锁定依赖:`actix-web = "4"`、`sqlx = "0.8"`、`tokio = "1"`(见 `Platform-Specifics.md` §5.2)

**Day 1 GATE**:
- [ ] 仓库能 clone
- [ ] CI 触发一次(空 build 跑通)
- [ ] aux-01 / 03 / 13 已 commit

### Day 2 - 协议冻结 + DB schema

**上午(4h)**:
- [ ] DB schema 设计(`crates/im-store/migrations/0001_init.sql`):
  ```sql
  CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    nickname VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );
  CREATE TABLE messages (
    id BIGSERIAL PRIMARY KEY,
    sender_id BIGINT NOT NULL REFERENCES users(id),
    receiver_id BIGINT NOT NULL REFERENCES users(id),
    type SMALLINT NOT NULL DEFAULT 1,  -- 1=text
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  );
  CREATE INDEX idx_messages_pair ON messages(sender_id, receiver_id, created_at DESC);
  ```
- [ ] aux-02 数据字典初版(列这 2 张表的字段)

**下午(4h)**:
- [ ] WS 协议定义(`crates/im-protocol/src/lib.rs`):
  - 客户端发:`auth` / `chat.publish` / `chat.history` / `ping`
  - 服务端发:`auth.ack` / `chat.publish.ack` / `chat.message` / `error` / `pong`
- [ ] REST API 定义(`crates/im-gateway/src/api.rs`):
  - `POST /api/v1/auth/register`
  - `POST /api/v1/auth/login`
  - `POST /api/v1/messages`
  - `GET /api/v1/messages?peer_id=X&limit=20&before=msg_id`
- [ ] 错误码写入 `aux-03`(已 Day 1 写好)
- [ ] **协议冻结**:在 PR 中标 `[PROTOCOL-FROZEN]`,任何变更需 PM 同意

**Day 2 GATE**:
- [ ] aux-02 初版 commit
- [ ] `im-protocol` crate 编译通过(占位)
- [ ] DB migration 可在本地 SQLite/Postgres 跑通
- [ ] **协议冻结 PR 已合并**

---

## Phase B:数据 + 协议骨架(Day 3-5)

### Day 3 - DB 层

- [ ] `im-store` crate(sqlx 0.9 + PostgreSQL 18.6):
  - `pub async fn create_user(email, password_hash, nickname) -> UserId`
  - `pub async fn get_user_by_email(email) -> Option<User>`
  - `pub async fn create_message(sender, receiver, payload) -> MessageId`
  - `pub async fn list_messages_between(a, b, before, limit) -> Vec<Message>`
- [ ] 单测覆盖(覆盖率 ≥ 60%,关键路径 ≥ 80%):
  - 创建 / 查重 / 边界(空表 / 超长 payload)
- [ ] 集成测:用 `docker compose up -d postgres:18.6` 跑 migration

**Day 3 GATE**:单测通过 + integration 跑通

### Day 4 - 认证 + REST(actix-web 4)

- [ ] `im-gateway` crate:
  - 路由注册(`/api/v1/auth/*`,`/api/v1/messages`)用 `App::new().service(...)`
  - JWT 签发(简单 HS256,密钥从 env)
  - 密码哈希(argon2)
  - 中间件:`actix-web-httpauth` 校验 Bearer Token
- [ ] 单测:登录 / 注册 / Token 校验 / 错误路径
- [ ] 集成测:用 `cargo test` 启 actix `test::TestServer`,curl 验证

**Day 4 GATE**:单测 + 集成测全绿

### Day 5 - WebSocket + 消息推送(actix-ws 0.3)

- [ ] WS 路由:`/ws`(鉴权用 query token)
- [ ] 连接管理:在线用户表(in-memory `DashMap<user_id, Vec<Sender>>`)
- [ ] 消息分发:收到 `chat.publish` → 写 DB → 推给 receiver 的连接
- [ ] 心跳:`ping` / `pong` 30s
- [ ] 单测:鉴权 / 重连 / 消息分发

**Day 5 GATE**:本地能用 `wscat` 发消息

---

## Phase C:实现完善(Day 6-9)

### Day 6-7 - 错误处理 + 日志 + 观测

- [ ] 全局 error handler(用 `aux-03` 的错误码)
- [ ] 日志:tracing + JSON output + `aux-09` 字段
- [ ] 健康检查:`/healthz` `/readyz`
- [ ] 配置:从 env + `.env`(详见 `aux-12`)

### Day 8-9 - 单测补齐 + CR

- [ ] 单测覆盖率 ≥ 60%(全),关键模块 ≥ 80%
- [ ] 每个 PR 至少 1 approve(Tech Lead)
- [ ] SAST:`cargo clippy -- -D warnings`,`cargo audit`
- [ ] 集成测剧本:`scripts/integration-test.sh`

**Day 9 GATE**:覆盖率达标 + CR 0 遗留 + 集成测全绿

---

## Phase D:联调(Day 10-11)

### Day 10 - 端到端 smoke

- [ ] 2 个终端(模拟 2 用户):
  - 用户 A 登录
  - 用户 B 登录
  - A 发消息给 B → B 实时收到
  - B 查历史 → 看到 A 的消息
- [ ] 压测基础(可选,看连接数):
  - 100 个并发连接 / 1k 消息 / 验证无 OOM
- [ ] Bug fix 集中修复

### Day 11 - 安全 + 边界

- [ ] 鉴权:无 token / 过期 token / 错误 token 都拒绝
- [ ] 注入:payload 含 `<script>` 等不执行
- [ ] 限流:1 用户 60 msg/min(简单 IP-based 限流)
- [ ] 数据:不能查别人的消息(测一遍)

**Day 11 GATE**:Smoke + 安全检查通过

---

## Phase E:部署(Day 12-13)

### Day 12 - K3s dev namespace

- [ ] 写 `k8s/dev/` 下的 manifests:
  - `namespace.yaml`
  - `postgres.yaml`(单实例)
  - `im-gateway.yaml`(Deployment + Service)
  - `ingress.yaml`(NodePort 或 ClusterIP)
- [ ] 在本地 K3s(用 k3d 或 K3s 单节点)跑通
- [ ] smoke 验证(同 Day 10)

### Day 13 - GitHub Actions 加 deploy job

- [ ] CI 加 `deploy-dev` job:push 后自动 deploy 到 K3s dev
- [ ] 用 `kubectl create secret` 配镜像 pull secret
- [ ] 触发一次完整 CI 验证(从 PR → 部署 → 验证)

**Day 13 GATE**:`git push` → 自动部署 → dev 可用

---

## Phase F:复盘(Day 14)

- [ ] 团队 retro(blameless):
  - 哪个环节超预期?哪个卡了?
  - 流程(本套 Workflow + RACI)是否需要调?
- [ ] 更新 `Project-Status.md`(记录实际工期 vs 计划)
- [ ] 下一步:Voice 集成 / 招人 / 性能 / 安全加固
- [ ] 决定:MVP 还要不要扩(群聊 / 推送 / 鉴权升级 OAuth2)

---

## 关键依赖

| 依赖 | 何时引入 | 备注 |
|---|---|---|
| PostgreSQL 14+ | Day 1 | dev 用 docker-compose |
| Redis 6+ | 不引入 | MVP 不需要缓存 |
| LiveKit | 不引入 | 留给 Voice 阶段 |
| 任何云服务 | 不引入 | 全部本地 K3s |
| 监控(Prometheus) | 不引入 | Day 14 后再考虑 |

---

## 验收标准(Day 14)

- [ ] 2 个浏览器 / 2 个终端可演示消息收发
- [ ] 所有 14 天的任务完成 / 显式放弃 + 记录
- [ ] 单测覆盖率 ≥ 60%
- [ ] CI 全绿(SAST + UT + 集成测)
- [ ] K3s dev namespace 跑通 + 监控到
- [ ] 团队 retro 完成 + 记录到 `Project-Status.md`

---

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| Rust 学习曲线 | TL 提前 1 周预热;不熟者先写小工具 |
| 14 天太紧 | 砍:群聊 / 文件 / 推送;推迟:Voice |
| 2-3 人一人离职阻塞 | 强制 2 人 approve / 文档优先 |
| GitHub Actions 慢 | 用 cache + 矩阵策略;考虑后续用自建 runner |

---

**维护**:本文档与 `Project-Status.md` 同步更新;每天 stand-up 时 review。
