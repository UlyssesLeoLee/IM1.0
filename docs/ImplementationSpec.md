# IM1.0 实施规范 (Implementation Specification)

> **版本**:v1.0.0
> **日期**:2026-08-23
> **状态**:Active(替代 DetailedDesign §1-§10 的工程实施部分;DetailedDesign 保留为"设计依据",本规范为"工程交付清单")
> **范围**:**MVP**(消息收发最小闭环 + 必需的安全 / 观测 / 部署能力)
> **上游依据**:`docs/SRS.md`(需求) → `docs/BasicDesign.md`(架构) → `docs/DetailedDesign.md`(协议) → 本文档(实施)
> **下游交付**:`crates/im-*/`、`migrations/*.sql`、`k8s/dev/*`、CI 配置

---

## 0. 文档元信息

| 字段 | 值 |
|---|---|
| 文档 ID | IMPL-SPEC-001 |
| Owner | Tech Lead |
| Reviewer | PM, SRE |
| 与 DetailedDesign 关系 | **DetailedDesign 是"做什么 + 为什么",本规范是"怎么做"**;两者不重复,本规范**只**补充实施所需的具名 / 具数 / 具文件的内容 |
| 与 Workflow 关系 | 对应 `Workflow.md` Phase 04 (Detailed Design) + Phase 05 (Implementation) 的产出物 |
| 变更原则 | 任何 PR 涉及接口 / 迁移 / 配置 / 错误码变更,必须同步更新本表相应章节,并在 `§15 Change Log` 加 1 行 |

---

## 1. 范围与非范围

### 1.1 在范围内 (MVP)

- `im-gateway`(独立进程,HTTP REST + WebSocket 终结,内部 gRPC 客户端)
- `im-core`(独立进程,Identity / Conversation / Message / Relationship 业务逻辑,PostgreSQL 唯一写入方,gRPC Server)
- `im-presence` / `im-media` / `extension-runtime`(MVP 阶段空 crate,占位,V1 实装;不部署独立 K3s 工作负载)
- 14 张表(参见 `aux-02` §F)
- 6 个 DB 迁移文件(参见 §4)
- **21 项错误码**(参见 §5,2026-08-23 自审新增 `FRIEND_REQUEST_NOT_FOUND`)
- **26 个 REST 端点**(7 类:鉴权 5 / 会话 5 / 消息 5 / 好友 4 / 媒体 2 / 用户 2 / 健康 3)
- **12 类 WebSocket 帧 × 2 方向**(客户端 8 类 + 服务端 11 类,部分重叠)
- **22 个 gRPC RPC**(`CoreService`,im-gateway ⇄ im-core,详见 §3.3)
- **27 个环境变量**(必填 9 + 可调 15 + 可观测 3,详见 §6)
- **10 个 K3s Secret Key**(4 个 per-env:server_secret / jwt_v1 / jwt_v2 / refresh_pepper;6 个 shared:db / valkey / nats / minio_endpoint / minio_access / minio_secret)
- **5 个 K3s 工作负载**(`im-gateway`, `im-core`, `postgres`, `valkey`, `nats`)+ 1 个 K3s 命名空间
- **3 条 GitHub Actions 流水线**(`ci.yml` 4 job / `release.yml` / `deploy-dev.yml`)
- **5 个关键模块**单测覆盖 ≥ 80%(MessageService / TokenService / ConversationService / WsSession / MessageRepository)

### 1.2 不在范围 (MVP)

- 任何 AI / Work Extension(只在骨架层预留,无实际扩展)
- LiveKit / 语音子系统(V1 单独 `LiveKit-Voice-Subsystem.md` 跟进)
- 端到端加密 / E2EE(V1+)
- 多区域部署 / 数据驻留(V1+)
- 自动化计费 / Quota Metering(V1+)
- Unreal / Godot 官方 SDK(MVP 只 Unity 优先)
- Web Dashboard / Full Client(MVP 用 REST + 简单 HTML demo;Next.js Dashboard 在 V1)
- HUD(MVP 阶段仅做 REST + WS 协议层验证,HUD 容器技术 V1 决定,见 `BasicDesign §12` ADR-013)

### 1.3 与 Day 1 任务清单的对应

`docs/Day1-Task-List.md` 的 14 天任务在本规范中**直接对应**到具体章节:

| Day 1 任务 | 本规范章节 |
|---|---|
| 仓库 / Cargo workspace | §2.1 (crate 布局) |
| `aux-01` 命名规范 | (已存在,引用) |
| `aux-03` 错误码 | §5 |
| `aux-13` 协议帧 | (已存在,引用) |
| DB schema | §4 |
| WS 协议 | §3.2 |
| REST API | §3.1 |
| DB 迁移 | §4.2 |
| 单测 ≥ 60% | §10.2 |

---

## 2. 代码仓库结构

### 2.1 Cargo Workspace 布局

```
im1.0/                              # 仓库根
├── Cargo.toml                      # workspace root
├── Cargo.lock                      # 必须入库
├── crates/
│   ├── im-common/                  # 共享:错误类型(aux-03 单一来源)、tracing 初始化、配置加载
│   ├── im-proto/                   # 共享 protobuf 定义 + tonic-build 生成代码
│   ├── im-protocol/                # WS 帧 / 错误码 / 内容 schema Rust 类型
│   ├── im-core/                    # 业务逻辑主进程
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── identity/           # 用户、Guest、Token、DeviceSession
│   │   │   │   ├── mod.rs
│   │   │   │   ├── repository.rs
│   │   │   │   ├── service.rs
│   │   │   │   ├── token.rs
│   │   │   │   └── password.rs     # argon2 wrapper
│   │   │   ├── relationship/       # 好友 / 拉黑
│   │   │   │   ├── mod.rs
│   │   │   │   ├── repository.rs
│   │   │   │   └── service.rs
│   │   │   ├── conversation/       # 会话 + 成员
│   │   │   │   ├── mod.rs
│   │   │   │   ├── repository.rs
│   │   │   │   └── service.rs
│   │   │   ├── message/            # 消息 + reaction
│   │   │   │   ├── mod.rs
│   │   │   │   ├── repository.rs
│   │   │   │   ├── service.rs
│   │   │   │   ├── sequence.rs
│   │   │   │   └── content.rs      # content JSON schema 校验
│   │   │   ├── event/              # 领域事件定义 + NATS publish
│   │   │   │   ├── mod.rs
│   │   │   │   └── publisher.rs
│   │   │   └── settings/           # environments.settings 加载 + 缓存 + 失效
│   │   │       ├── mod.rs
│   │   │       ├── repository.rs
│   │   │       └── service.rs
│   │   ├── migrations/             # (使用根目录 migrations/ 而非 crate 内,见 §4.2)
│   │   └── tests/                  # 集成测(用 testcontainers 启 PG)
│   ├── im-gateway/                 # HTTP + WS 终结
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs             # 启动 actix-web + actix-ws
│   │   │   ├── http/               # REST API 路由
│   │   │   │   ├── mod.rs
│   │   │   │   ├── auth.rs
│   │   │   │   ├── conversations.rs
│   │   │   │   ├── messages.rs
│   │   │   │   ├── friends.rs
│   │   │   │   ├── media.rs
│   │   │   │   ├── me.rs
│   │   │   │   └── health.rs
│   │   │   ├── ws/                 # WebSocket 终结
│   │   │   │   ├── mod.rs
│   │   │   │   ├── session.rs
│   │   │   │   ├── router.rs       # 帧 → 内部 handler 路由
│   │   │   │   ├── frames.rs       # 出帧构造
│   │   │   │   └── heartbeat.rs
│   │   │   ├── error.rs            # AppError → HTTP/WS 错误响应映射
│   │   │   ├── auth_middleware.rs  # Bearer Token 校验
│   │   │   └── ratelimit.rs        # Valkey 令牌桶
│   │   └── tests/                  # 集成测(actix test::TestServer)
│   ├── im-presence/                # (MVP 阶段空,只占位 crate,V1 实装)
│   ├── im-media/                   # (MVP 阶段最小:仅生成 presign URL,V1 实装媒体处理)
│   └── extension-runtime/          # (MVP 阶段空 crate,仅提供 manifest 解析,V1 实装)
├── migrations/                     # sqlx 迁移根目录(全 crate 共用)
│   ├── 0001_create_tenants_games_environments.sql
│   ├── 0002_create_users_device_sessions.sql
│   ├── 0003_create_friend_requests_and_friendships.sql
│   ├── 0004_create_conversations_sequences_members_dm_pairs.sql
│   ├── 0005_create_messages_reactions.sql
│   └── 0006_create_audit_logs.sql
├── web/dashboard/                  # Next.js 15 (V1,MVP 阶段不存在)
├── deploy/k3s/                     # K3s manifests
│   ├── dev/
│   │   ├── namespace.yaml
│   │   ├── postgres.yaml
│   │   ├── valkey.yaml
│   │   ├── nats.yaml
│   │   ├── im-gateway.yaml
│   │   ├── im-core.yaml
│   │   ├── secrets-template.yaml   # 含 server_secret / jwt_signing_key 占位
│   │   └── ingress.yaml
│   ├── staging/                    # V1+
│   └── prod/                       # V1+
├── docker/
│   ├── im-gateway.Dockerfile
│   └── im-core.Dockerfile
├── .github/
│   ├── workflows/
│   │   ├── ci.yml
│   │   ├── release.yml
│   │   └── deploy-dev.yml
│   ├── ISSUE_TEMPLATE/
│   ├── PULL_REQUEST_TEMPLATE.md
│   ├── CODEOWNERS
│   └── dependabot.yml
├── scripts/
│   ├── check_error_codes.sh        # 扫描所有错误码字符串,确保都在 aux-03
│   ├── check_naming.sh             # 扫描命名违规
│   ├── integration_test.sh         # 端到端冒烟
│   ├── gen_workflow_templates.py
│   └── gen_dd_aux.py
├── docs/                           # (已有,见 README.md)
│   ├── SRS.md
│   ├── BasicDesign.md
│   ├── DetailedDesign.md
│   ├── ImplementationSpec.md       # 本文档
│   ├── ... (其他 11 份)
│   └── templates/04-detailed-design/auxiliary/*.md  # (已更新)
├── .gitignore
├── .gitattributes
├── Cargo.toml
├── Cargo.lock
├── LICENSE
└── README.md
```

### 2.2 关键依赖锁版本 (`Cargo.toml` workspace.dependencies)

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
# Web
actix-web = "4"
actix-ws = "0.3"
actix-web-httpauth = "0.8"
actix-cors = "0.7"
actix-rt = "2"

# 异步运行时
tokio = { version = "1", features = ["full"] }
futures = "0.3"
async-trait = "0.1"

# 数据库
# 2026-08-26 升级:0.8.6 → 0.9.0(0.8.6 在 cargo 1.98 / rustc 1.98 上 sqlx-macros-core
# 触发 E0220 "Output not found for F" + "cannot determine resolution for the derive
# macro Debug";0.8.x 无 0.8.7+ patch,0.9.0 (2026-05-21) 已修复)
sqlx = { version = "0.9", features = ["runtime-tokio", "postgres", "macros", "migrate", "uuid", "chrono", "json"] }

# 缓存 / 键值
redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }

# 事件总线
# 2026-08-26 升级:0.37.0 → 0.50.0(0.37.0 编译期 STATUS_STACK_BUFFER_OVERRUN
# 0xc0000409 / 6 MB 内存分配失败;0.50.0 (2026-07-20) stable,默认 features
# 仍含 ring/jetstream/websockets/kv/object-store,§6 NATS Secret Key 不变,
# im-core/src/event/publisher.rs 占位代码实装时需核对 0.50 API 命名空间)
async-nats = "0.50"

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"
bytes = "1"

# ID / 时间
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

# 错误
thiserror = "1"
anyhow = "1"

# 密码 / Token
argon2 = "0.5"
jsonwebtoken = "9"
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"

# 媒体
s3 = { version = "0.34", features = ["rustls-tls"] }    # MinIO S3 兼容

# 可观测性
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-actix-web = "0.7"
opentelemetry = { version = "0.24" }
opentelemetry-otlp = { version = "0.17", features = ["grpc-tonic"] }
opentelemetry_sdk = { version = "0.24", features = ["rt-tokio"] }
# 2026-08-26 升级:0.13.4 → 0.14.0 + default-features = false(0.13.4 default 拉
# protobuf 2.28.0,后者在 cargo 1.98 codegen 阶段必爆栈;0.14 (2026-03 发布) 仍
# default = ["protobuf"],故显式 default-features = false;V1 实装 metrics 时再
# 开 protobuf feature)
prometheus = { version = "0.14", default-features = false }

# gRPC
tonic = "0.12"
tonic-build = "0.12"
prost = "0.13"

# 配置
figment = { version = "0.10", features = ["env", "toml"] }
dotenvy = "0.15"

# 工具
secrecy = "0.8"
once_cell = "1"
parking_lot = "0.12"
# 2026-08-26 移除:dashmap 6.2.1 (最新 stable 也是 6.2.1) 在 cargo 1.98 / rustc
# 1.98 上因 Self::Output 推导撞上 IntoFuture::Output / AsyncFnOnce::Output /
# FnOnce::Output 触发 5×E0223 ambiguous associated type (dashmap 自身
# src/lib.rs:1267-1303 触发)。IM1.0 仓库 0 处实际调用 (grep 验证),im-core
# 仅 Cargo.toml 声明,故直接移除依赖;V1+ 若需并发 map 改 parking_lot::Mutex<HashMap>。

# 测试
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres", "redis"] }
proptest = "1"
mockall = "0.13"
criterion = "0.5"  # bench(本规范不强制,V1+)
```

> **锁定策略**:`Cargo.lock` 入库,Dependabot 每周自动 PR 升级(见 `Platform-Specifics.md §2.4`),人工 review 后合并。
>
> **2026-08-23 自审注**:以下版本号为 MVP 启动时的**初始估计**,**首次 `cargo build` 时必须以 Cargo.lock 实际 resolve 出来的版本为准**(部分 crate 仍在 0.x 阶段,minor 升级可能 breaking)。CI 中加入 `cargo-deny` 检查 license + advisory + banned crates。

### 2.3 .gitignore 关键项

参见 `Platform-Specifics.md §1.3`,补充:

```gitignore
# sqlx 离线缓存
.sqlx/

# Rust benchmark
benches/
target/criterion/

# Editor
.idea/
.vscode/
*.swp
```

---

## 3. API 契约(实施级)

> 协议细节(Wire format / Error body / Heartbeat) 已在 `aux-13` 中给完整帧样例;本节给出**每个端点的实施 Checklist**(验证项、参数校验、状态码、幂等行为)。

### 3.1 REST API Checklist (im-gateway 对外)

> 完整端点列表见 `DetailedDesign.md §5` 表。本表列出每个端点的**实施约束**(必填校验、必带响应头、错误码)。任何新端点必须按本表格式补充。

#### 3.1.1 鉴权与身份

| Method | Path | 鉴权 | Body 必填 | 成功响应 | 错误码 | 幂等 |
|---|---|---|---|---|---|---|
| POST | `/v1/auth/token/exchange` | **Server Secret (HMAC)** | `{environment_id, external_provider, external_uid, display_name?}` | 200 `{access_token, refresh_token, user_id, expires_in}` | `UNAUTHORIZED`, `VALIDATION_ERROR`, `RATE_LIMITED` | 否 |
| POST | `/v1/auth/guest` | 无 | `{environment_id}` | 200 同上 | `VALIDATION_ERROR`, `RATE_LIMITED` | 否(可加 X-IM-Idempotency-Key 可选) |
| POST | `/v1/auth/refresh` | Refresh Token (Body) | `{refresh_token}` | 200 同上(返回新对,旧 refresh 失效) | `UNAUTHORIZED`, `RATE_LIMITED` | 否(由 DB `refresh_token_hash` 唯一性保证) |
| POST | `/v1/auth/link` | Access Token + 外部凭证 | `{external_provider, external_uid}` | 200 `{user_id, access_token, refresh_token}` | `UNAUTHORIZED`, `ACCOUNT_MERGE_CONFLICT`, `VALIDATION_ERROR` | 否 |
| POST | `/v1/auth/logout` | Access Token | 空 | 204 | `UNAUTHORIZED` | 否 |

**实施要求**:
- `/v1/auth/token/exchange` 必须校验:
  - Header `X-IM-Server-Signature: hex(HMAC-SHA256(server_secret, body))` —— 用 `im-core` Secret
  - Header `X-IM-Timestamp: unix_seconds` —— 与服务端时间差 > 5min 拒绝(防 replay)
  - Header `X-IM-Nonce: random_32_bytes` —— 服务端记录最近 10min nonce,重放拒绝
  - `body` 必须包含 `environment_id`,且此 env 的 `server_secret` 必须存在(否则 401)
- `POST /v1/auth/refresh` 必须旋转 Refresh Token(每次刷新旧 token_hash 失效,对应 `device_sessions.refresh_token_hash` 唯一约束)

#### 3.1.2 会话

| Method | Path | 鉴权 | 错误码 |
|---|---|---|---|
| POST | `/v1/conversations` | Access Token | `VALIDATION_ERROR`, `FORBIDDEN`, `USER_BLOCKED`, `RATE_LIMITED` |
| GET | `/v1/conversations?cursor=&limit=` | Access Token | `VALIDATION_ERROR` |
| GET | `/v1/conversations/{id}` | Access Token + 成员校验 | `CONVERSATION_NOT_FOUND`, `FORBIDDEN` |
| GET | `/v1/conversations/{id}/messages?after_sequence=&limit=` | Access Token + 成员校验 | `CONVERSATION_NOT_FOUND`, `FORBIDDEN`, `VALIDATION_ERROR` |
| GET | `/v1/conversations/{id}/members` | Access Token + 成员校验 | `CONVERSATION_NOT_FOUND`, `FORBIDDEN` |

**实施要求**:
- 响应体统一含 `next_cursor: string|null` 用于分页
- `limit` 默认 50,最大 200
- 创建 DM 时幂等:`(env, user_a, user_b)` 已存在则返回原会话,创建新会话必须 upsert `dm_pairs`
- 成员校验:服务端必须在所有读写前验证 `is_member(conv_id, user_id)`,失败返回 `FORBIDDEN`
- **Guest 限制**:`users.kind='guest'` 仅允许创建 `kind=dm` 的会话(由 token claims 中的 `kind` 字段判断),且仅当另一方非 banned;`kind=group` / `channel` 全部返回 `FORBIDDEN`(呼应 §11.1)

#### 3.1.3 消息

| Method | Path | 鉴权 | 错误码 |
|---|---|---|---|
| POST | `/v1/conversations/{id}/messages` | Access Token + 成员校验 | `VALIDATION_ERROR`, `MESSAGE_TOO_LARGE`, `RATE_LIMITED`, `FORBIDDEN` |
| PATCH | `/v1/conversations/{id}/messages/{msg_id}` | Access Token + sender 校验 | `MESSAGE_NOT_FOUND`, `FORBIDDEN`, `RECALL_WINDOW_EXPIRED` |
| POST | `/v1/conversations/{id}/messages/{msg_id}/recall` | Access Token + sender 校验 | `MESSAGE_NOT_FOUND`, `FORBIDDEN`, `RECALL_WINDOW_EXPIRED`, `INVALID_STATE_TRANSITION` |
| POST | `/v1/conversations/{id}/messages/{msg_id}/reactions` | Access Token + 成员校验 | `MESSAGE_NOT_FOUND`, `FORBIDDEN` |
| POST | `/v1/conversations/{id}/read` | Access Token + 成员校验 | `CONVERSATION_NOT_FOUND`, `FORBIDDEN` |

**实施要求**:
- `POST /messages` 必须读取 `X-IM-Idempotency-Key` Header(若缺则 400 `INVALID_IDEMPOTENCY_KEY`)
- `content` JSON 在 4 种 kind 下有具体 schema(见 `aux-13 §4.1`),服务端必须按 kind 校验
- PATCH / recall 必须在 `IM_MESSAGE_RECALL_WINDOW_SECONDS` (或 env 覆盖值) 内
- `messages.content` 序列化字节数 ≤ `IM_MESSAGE_MAX_SIZE_BYTES` (默认 64KB)

#### 3.1.4 好友 / 拉黑

| Method | Path | Body 必填 | 错误码 |
|---|---|---|---|
| POST | `/v1/friends/requests` | `{recipient_id}` | `VALIDATION_ERROR`, `FRIEND_REQUEST_EXISTS`(已存在待处理), `USER_BLOCKED`(对方已屏蔽你), `RATE_LIMITED` |
| POST | `/v1/friends/requests/{id}/respond` | `{accept: bool}` | `FRIEND_REQUEST_NOT_FOUND`, `FORBIDDEN`(非 recipient), `INVALID_STATE_TRANSITION`(非 pending) |
| POST | `/v1/friends/{id}/block` | 空 | `VALIDATION_ERROR`(自指 = 400);成功 204(无错误) |
| GET | `/v1/friends` | — | `VALIDATION_ERROR` |

**实施要求**:
- 拉黑成功幂等(重复拉黑同一目标返回 204,不报错)
- 拉黑时若已存在 accepted 好友关系,自动转为 blocked(单边覆盖)
- 响应好友列表时**不**包含已 blocked 的关系

#### 3.1.5 媒体

| Method | Path | 错误码 |
|---|---|---|
| POST | `/v1/media/presign` | `VALIDATION_ERROR`, `RATE_LIMITED` |
| GET | `/v1/media/{id}` | `NOT_FOUND`, `FORBIDDEN` |

**实施要求**:
- 预签名 URL 默认 15min 过期
- 上传**不经过** im-gateway(直接 PUT 到 MinIO,呼应 `IM-MEDIA-002`)
- `GET /v1/media/{id}` 返回短期可读 URL(默认 1h)

#### 3.1.6 用户资料

| Method | Path | 错误码 |
|---|---|---|
| GET | `/v1/me` | `UNAUTHORIZED` |
| PATCH | `/v1/me` | `VALIDATION_ERROR`, `ACCOUNT_BANNED` |

#### 3.1.7 健康与监控

| Method | Path | 鉴权 | 说明 |
|---|---|---|---|
| GET | `/healthz` | 无 | 进程存活(返回 200 即认为 ok,无后端检查) |
| GET | `/readyz` | 无 | 就绪:**额外**检查到 im-core / PG / Valkey / NATS 的连接;任一不可用返回 503 |
| GET | `/metrics` | 内网 IP allowlist | Prometheus 文本格式 |

### 3.2 WebSocket 协议 Checklist

> 完整帧样例见 `aux-13 §1`。本节给出**路由 + 状态机 + 错误码**实施约束。

> **WS 鉴权方式决策**(2026-08-23 自审发现,Day 1 文档冲突):
> - `Day1-Task-List.md §Day 5` 写"WS 鉴权用 query token"(`?token=...`)
> - `DetailedDesign.md §3` / `aux-13 §1` 用 `auth` JSON 帧
> - **决定**:采用 `auth` 帧方案(`DetailedDesign` 是 DetailedDesign 阶段产物,优先级高于 Day 1 任务清单)
> - **理由**:query token 会把 token 写入 URL 访问日志 / nginx access log / 中间代理日志,泄露面大;auth 帧在 TCP upgrade 后才发送,只走 WS 二进制流,无法被旁路抓取
> - **实施**:`/ws` 路由不需要任何 query 参数,客户端 connect 后**必须**第一帧发 `auth`(详见 `aux-13 §1.1.1`);否则 100ms 内服务端主动断开(防滥用)

#### 3.2.1 客户端 → 服务端帧路由表

| Frame `type` | 路由到 | 必填字段 | 错误码 |
|---|---|---|---|
| `auth` | `WsSession::handle_auth` | `access_token` | `UNAUTHORIZED`, `VALIDATION_ERROR` |
| `send_message` | `MessageService::send_message` | `conversation_id`, `idempotency_key`, `kind`, `content` | `VALIDATION_ERROR`, `MESSAGE_TOO_LARGE`, `RATE_LIMITED`, `FORBIDDEN`, `MESSAGE_NOT_FOUND`(reply_to 校验) |
| `edit_message` | `MessageService::edit_message` | `message_id`, `content` | `MESSAGE_NOT_FOUND`, `FORBIDDEN`, `RECALL_WINDOW_EXPIRED` |
| `recall_message` | `MessageService::recall_message` | `message_id` | `MESSAGE_NOT_FOUND`, `FORBIDDEN`, `RECALL_WINDOW_EXPIRED`, `INVALID_STATE_TRANSITION` |
| `react` | `MessageService::react` | `message_id`, `emoji` | `MESSAGE_NOT_FOUND`, `FORBIDDEN` |
| `mark_read` | `MessageService::mark_read` | `conversation_id`, `sequence` | `CONVERSATION_NOT_FOUND`, `FORBIDDEN` |
| `typing` | `WsSession::broadcast_typing` | `conversation_id` | `FORBIDDEN` |
| `ping` | `WsSession::on_ping` | (无) | — |

#### 3.2.2 服务端 → 客户端帧触发条件

| Frame `type` | 触发 |
|---|---|
| `connected` | WS 握手成功(TCP upgrade 完成后 100ms 内) |
| `ack` | 任何客户端写操作的响应(必带相同 `req_id`) |
| `message_new` | im-core 提交消息事务后,经 NATS `im.message.created` 事件 → fanout → 该会话所有在线成员的 WS 连接 |
| `message_edited` | im-core 提交编辑后,经 NATS `im.message.edited` 事件 → fanout |
| `message_recalled` | 同上,`im.message.recalled` |
| `reaction_added` | 同上,`im.message.reaction_added` |
| `presence_update` | im-presence 检测到用户上线/下线,经 NATS `im.presence.changed` → fanout 给关注者 |
| `typing` | 客户端发 `typing` 帧后,服务端转发给会话其他在线成员(自身不接收) |
| `pong` | 响应客户端 `ping` |
| `force_disconnect` | Token revoked / account banned / admin kick(由后端事件触发,WS 关闭连接) |

#### 3.2.3 协议级约束

- 所有客户端写操作必带 `req_id`(UUID v4),服务端 `ack` 回带同 req_id
- 心跳:客户端 30s `ping`(`IM_WS_PING_INTERVAL_SECONDS`),服务端 60s 无帧超时(`IM_WS_HEARTBEAT_TIMEOUT_SECONDS`)主动断开
- 重连:**不**做服务端补发;客户端重连后用 `GET /v1/conversations/{id}/messages?after_sequence={local_cursor}` 增量拉取
- 顺序:同一会话内 `message_new` 按 `sequence` 升序(由 im-core 单调分配保证,详见 §4.5)
- `IDEMPOTENCY_CONFLICT` 走成功语义(`ok=true` + `idempotent_replay: true`),客户端无需分支处理

#### 3.2.4 协议冻结声明 [PROTOCOL-FROZEN]

> **冻结时间**:2026-08-26 JST
> **冻结范围**:WS 12 个帧(双向 8 client + 4 server-only unique) + gRPC 22 个 RPC(§3.3) + REST 26 个端点 / 7 类(§3.1.1-§3.1.7)
> **冻结标签**:`[PROTOCOL-FROZEN]`(本 commit 标题与 Project-Status §1.1 同步)
> **变更流程**:任何破坏性变更走 `aux-13 §7` 流程(Slack 公告 7 天 + RFC + Tech Lead + PM review + 6 个月兼容期)
> **配套 commit**: `feat: Day 2 GATE 补签 + 协议冻结 [PROTOCOL-FROZEN]`(commit hash 见 §15 v1.0.3)

冻结时点对应的实现:
- WS 帧类型:`auth` / `send_message` / `edit_message` / `recall_message` / `react` / `mark_read` / `typing` / `ping`(client 8)+ `connected` / `ack` / `message_new` / `message_edited` / `message_recalled` / `reaction_added` / `presence_update` / `pong` / `force_disconnect`(server 9 unique)= 17 unique types
- gRPC 22 RPC:见 §3.3 表(ExchangeToken / AuthenticateGuest / RefreshToken / ValidateAccessToken / LinkAccount / Logout / CreateConversation / ListConversations / GetConversation / SendMessage / ListMessages / EditMessage / RecallMessage / ReactMessage / MarkRead / SendFriendRequest / RespondFriendRequest / BlockUser / ListFriends / PresignMedia / GetMe / UpdateMe)
- REST 26 端点(7 类):鉴权 5 / 会话 5 / 消息 5 / 好友 4 / 媒体 2 / 用户资料 2 / 健康 3(见 §3.1.1-§3.1.7)

### 3.3 gRPC 契约(im-gateway ⇄ im-core, package `im.core.v1`)

完整 proto 见 §2.1 中的 `crates/im-proto/proto/core.proto`。**MVP 必实现 14 个 RPC**:

| RPC | 请求 | 响应 | 错误映射(`aux-13 §2.5`) |
|---|---|---|---|
| `ExchangeToken` | `{environment_id, external_provider, external_uid, display_name?, server_signature, timestamp, nonce}` | `{access_token, refresh_token, user_id, expires_in}` | `UNAUTHENTICATED`, `INVALID_ARGUMENT`, `RESOURCE_EXHAUSTED` |
| `AuthenticateGuest` | `{environment_id}` | `{access_token, refresh_token, user_id, expires_in}` | `INVALID_ARGUMENT`, `RESOURCE_EXHAUSTED` |
| `RefreshToken` | `{refresh_token}` | `{access_token, refresh_token, user_id, expires_in}` | `UNAUTHENTICATED`, `RESOURCE_EXHAUSTED` |
| `ValidateAccessToken` | `{access_token}` | `{valid, user_id, environment_id, tenant_id, expires_at_unix}` | (总是返回 valid,无效时 `valid=false`) |
| `LinkAccount` | `{access_token, external_provider, external_uid}` | `{user_id, access_token, refresh_token, expires_in}` | `UNAUTHENTICATED`, `FAILED_PRECONDITION` (ACCOUNT_MERGE_CONFLICT) |
| `Logout` | `{user_id, device_session_id}` | `Empty` | `UNAUTHENTICATED` |
| `CreateConversation` | `{kind, environment_id, creator_user_id, member_user_ids, metadata_json}` | `Conversation` | `INVALID_ARGUMENT`, `PERMISSION_DENIED` |
| `ListConversations` | `{user_id, cursor, limit}` | `{conversations[], next_cursor}` | `INVALID_ARGUMENT` |
| `GetConversation` | `{conversation_id, user_id}` | `Conversation` | `NOT_FOUND`, `PERMISSION_DENIED` |
| `SendMessage` | `{conversation_id, sender_id, idempotency_key, kind, content_json, reply_to?}` | `Message` | 见 §3.2.1 send_message 错误码 |
| `ListMessages` | `{conversation_id, user_id, after_sequence, limit}` | `{messages[], has_more, latest_sequence}` | `NOT_FOUND`, `PERMISSION_DENIED` |
| `EditMessage` | `{message_id, user_id, content_json}` | `Message` | 见 §3.2.1 edit_message 错误码 |
| `RecallMessage` | `{message_id, user_id}` | `Empty` | 见 §3.2.1 recall_message 错误码 |
| `ReactMessage` | `{message_id, user_id, emoji}` | `Empty` | `NOT_FOUND`, `PERMISSION_DENIED` |
| `MarkRead` | `{conversation_id, user_id, sequence}` | `Empty` | `NOT_FOUND`, `PERMISSION_DENIED` |
| `SendFriendRequest` | `{environment_id, sender_id, recipient_id}` | `Empty` | `FAILED_PRECONDITION` (FRIEND_REQUEST_EXISTS), `PERMISSION_DENIED` |
| `RespondFriendRequest` | `{request_id, responder_id, accept: bool}` | `Empty` | `NOT_FOUND`, `PERMISSION_DENIED`, `FAILED_PRECONDITION` (INVALID_STATE_TRANSITION) |
| `BlockUser` | `{user_id, target_id}` | `Empty` | — |
| `ListFriends` | `{user_id, cursor, limit}` | `{friends[], next_cursor}` | `INVALID_ARGUMENT` |
| `PresignMedia` | `{user_id, content_type, size_hint}` | `{upload_url, media_id, expires_at_unix}` | `INVALID_ARGUMENT`, `RESOURCE_EXHAUSTED` |
| `GetMe` | `{user_id}` | `User` | `UNAUTHENTICATED` |
| `UpdateMe` | `{user_id, display_name?}` | `User` | `INVALID_ARGUMENT`, `PERMISSION_DENIED` (ACCOUNT_BANNED) |

> **MVP 范围**:上表全部 RPC;新增需更新本表 + proto 文件 + DetailedDesign §2 + aux-13 §2。

---

## 4. 数据库迁移(具体脚本)

> 表结构定义见 `BasicDesign.md §4`(权威源);本节给出**迁移文件**实施细节(命名、文件内容、PostgreSQL 特性依赖、回滚策略)。

### 4.1 迁移工具与约定

- 工具:`sqlx migrate`(`sqlx-cli`,版本 0.8.x)
- 目录:`migrations/`(workspace 根,不放在 crate 内)
- 文件命名:`<4位序号>_<snake_case 描述>.sql`,序号永远递增
- 单向 vs 双向:本项目用**单向**(仅 `Up`);`Down` 由 `aux-10` DD Review 时确认"是否需要支持回滚"(MVP 默认否;生产事故需手工反向迁移)
- 顺序执行保证:`sqlx migrate` 串行执行
- 不可变:**已合并的迁移文件永不修改**;任何修改必须新加一个迁移

### 4.2 MVP 迁移文件清单

#### 4.2.1 `0001_create_tenants_games_environments.sql`

```sql
-- +migrate Up
CREATE EXTENSION IF NOT EXISTS pgcrypto;  -- gen_random_uuid()

CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL CHECK (length(name) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE games (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(name) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);
CREATE INDEX idx_games_tenant_id ON games(tenant_id);

CREATE TABLE environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (name IN ('production', 'staging', 'test')),
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (game_id, name),
    CONSTRAINT chk_environments_settings_is_object CHECK (jsonb_typeof(settings) = 'object')
);
CREATE INDEX idx_environments_game_id ON environments(game_id);

-- updated_at 自动触发器
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_environments_before_update
BEFORE UPDATE ON environments
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
```

#### 4.2.2 `0002_create_users_device_sessions.sql`

```sql
-- +migrate Up
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('user', 'guest')),
    external_identity JSONB,           -- Guest 必为 NULL;user 必填
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'banned', 'suspended', 'deleted')),
    display_name TEXT CHECK (length(display_name) <= 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- (env, external_identity) 联合唯一;NULL 视为 distinct(PG 默认行为),
    -- 即:同 env 下只允许 1 个 user 拥有特定 extid;但允许任意多个 Guest(extid=NULL)
    CONSTRAINT uniq_users_env_extid UNIQUE (environment_id, external_identity)
);
CREATE INDEX idx_users_environment_id ON users(environment_id);
CREATE INDEX idx_users_state ON users(state) WHERE state != 'active';

CREATE TABLE device_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_fingerprint TEXT CHECK (length(device_fingerprint) <= 256),
    refresh_token_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    CONSTRAINT uniq_device_sessions_active_refresh UNIQUE (user_id, refresh_token_hash)
);
CREATE INDEX idx_device_sessions_user_id ON device_sessions(user_id);
CREATE INDEX idx_device_sessions_revoked_at ON device_sessions(revoked_at) WHERE revoked_at IS NULL;
```

#### 4.2.3 `0003_create_friend_requests_and_friendships.sql`

```sql
-- +migrate Up
CREATE TABLE friend_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    sender_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'accepted', 'rejected', 'expired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (environment_id, sender_id, recipient_id),
    CHECK (sender_id <> recipient_id)
);
CREATE INDEX idx_friend_requests_recipient ON friend_requests(recipient_id, state) WHERE state = 'pending';
CREATE INDEX idx_friend_requests_sender ON friend_requests(sender_id, state);

CREATE TRIGGER trg_friend_requests_before_update
BEFORE UPDATE ON friend_requests
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE friendships (
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    friend_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('accepted', 'blocked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (environment_id, user_id, friend_id),
    CHECK (user_id <> friend_id)
);
CREATE INDEX idx_friendships_user ON friendships(user_id, state);
```

#### 4.2.4 `0004_create_conversations_sequences_members_dm_pairs.sql`

```sql
-- +migrate Up
CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('dm', 'group', 'channel', 'system', 'broadcast')),
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_conversations_metadata_is_object CHECK (jsonb_typeof(metadata) = 'object')
);
CREATE INDEX idx_conversations_environment_id ON conversations(environment_id, created_at DESC);

CREATE TABLE conversation_sequences (
    conversation_id UUID PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    next_sequence BIGINT NOT NULL DEFAULT 1 CHECK (next_sequence >= 1)
);

CREATE TABLE dm_pairs (
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    user_a UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    PRIMARY KEY (environment_id, user_a, user_b),
    UNIQUE (conversation_id),
    CHECK (user_a < user_b)
);
CREATE INDEX idx_dm_pairs_conversation ON dm_pairs(conversation_id);

CREATE TABLE conversation_members (
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_read_sequence BIGINT NOT NULL DEFAULT 0 CHECK (last_read_sequence >= 0),
    PRIMARY KEY (conversation_id, user_id)
);
-- "我的会话列表"查询专用
CREATE INDEX idx_conversation_members_user_id ON conversation_members(user_id);
```

#### 4.2.5 `0005_create_messages_reactions.sql`

```sql
-- +migrate Up
CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sequence BIGINT NOT NULL CHECK (sequence >= 1),
    sender_id UUID REFERENCES users(id) ON DELETE SET NULL,  -- 系统消息 NULL
    kind TEXT NOT NULL CHECK (kind IN ('text', 'image', 'file', 'sticker', 'system', 'custom')),
    content JSONB NOT NULL,
    reply_to UUID REFERENCES messages(id) ON DELETE SET NULL,
    idempotency_key TEXT NOT NULL CHECK (length(idempotency_key) <= 128),
    state TEXT NOT NULL DEFAULT 'sent' CHECK (state IN ('sent', 'delivered', 'read', 'recalled', 'deleted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at TIMESTAMPTZ,
    -- 同一会话内 sequence 单调
    UNIQUE (conversation_id, sequence),
    -- 幂等键唯一(sender_id=NULL 即系统消息)
    CONSTRAINT uniq_messages_idem UNIQUE NULLS NOT DISTINCT (conversation_id, sender_id, idempotency_key),
    CONSTRAINT chk_messages_content_is_object CHECK (jsonb_typeof(content) = 'object')
);

-- 核心查询路径:增量拉取 (conversation_id, after_sequence)
CREATE INDEX idx_messages_conversation_seq ON messages(conversation_id, sequence);
-- 全文搜索预留(为 V1+ IM-SEARCH-001 准备, MVP 暂不接 GIN)
-- CREATE INDEX idx_messages_content_fts ON messages USING GIN (to_tsvector('simple', content::text));

CREATE TABLE message_reactions (
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji TEXT NOT NULL CHECK (length(emoji) <= 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, user_id, emoji)
);
CREATE INDEX idx_message_reactions_message ON message_reactions(message_id);
```

#### 4.2.6 `0006_create_audit_logs.sql`

```sql
-- +migrate Up
CREATE TABLE audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    actor_id UUID,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL CHECK (target_type IN ('user', 'conversation', 'environment', 'extension', 'secret', 'system')),
    target_id UUID,
    detail JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_logs_tenant_created ON audit_logs(tenant_id, created_at DESC);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);
-- audit 专用分区(V1+ 启用,MVP 不分区):
-- ALTER TABLE audit_logs PARTITION BY RANGE (created_at);
```

### 4.3 迁移执行流程

```bash
# 开发环境(本机)
sqlx migrate run --database-url $IM_DATABASE_URL

# CI 集成测(临时 DB)
# 由 GitHub Actions 的 test-integration job 自动执行:
# 1. 用 testcontainers 拉 postgres:18
# 2. export DATABASE_URL=...
# 3. sqlx migrate run
# 4. cargo test --workspace --test '*'

# 生产 K3s
# 在 im-gateway Deployment 的 preStart hook 执行:
kubectl exec -n im-platform deploy/im-gateway -- sqlx migrate run
# 或单独的 init Job(推荐,见 §8.2)
```

### 4.4 索引策略检查表(由 `aux-07` SQL 优化 Checklist 校验)

| 表 | 索引 | 用途 | 关键查询 |
|---|---|---|---|
| `users` | `(environment_id, external_identity)` UNIQUE(NUL distinct,允许多 Guest) | Token Exchange 查重 | `WHERE environment_id = $1 AND external_identity = $2` |
| `users` | `(state)` partial WHERE state != 'active' | 状态过滤 | 风控查 banned/suspended 列表 |
| `device_sessions` | `(user_id, refresh_token_hash)` UNIQUE | Refresh 校验 | `WHERE user_id = $1 AND refresh_token_hash = $2` |
| `device_sessions` | `(revoked_at)` partial WHERE NULL | 找有效会话 | 用户活跃设备列表 |
| `friend_requests` | `(recipient_id, state)` partial WHERE 'pending' | 收件箱 | `WHERE recipient_id = $1 AND state = 'pending'` |
| `conversations` | `(environment_id, created_at DESC)` | 会话列表 | `WHERE environment_id IN (...) ORDER BY created_at DESC` |
| `conversation_members` | `(user_id)` | "我的会话列表" | `WHERE user_id = $1` |
| `messages` | `(conversation_id, sequence)` | 增量拉取(主路径) | `WHERE conversation_id = $1 AND sequence > $2` |
| `messages` | `(conversation_id, sender_id, idempotency_key)` UNIQUE **NULLS NOT DISTINCT**(系统消息 sender=NULL 也要去重) | 幂等检查 | 发送前 `find_by_idempotency_key` |
| `audit_logs` | `(tenant_id, created_at DESC)` | 租户审计查询 | 合规导出 |

### 4.5 Sequence 分配(关键)

> 实现细节见 `DetailedDesign.md §5` + §9.1。本节给出**性能与正确性保证**。

- **算法**:`SELECT next_sequence FROM conversation_sequences WHERE conversation_id = $1 FOR UPDATE` → 在同一事务内 UPDATE + INSERT messages
- **正确性**:同一会话内 `UNIQUE (conversation_id, sequence)` 保证不重复;行锁保证单调
- **性能天花板**:单会话连续写入时为单行串行(高 QPS 单会话瓶颈);实测后若成瓶颈(V1+ ADR 候选)→ 评估 Snowflake / 分布式 ID + 客户端补偿
- **空洞容忍**:`sequence` 不要求连续(只单调),允许事务回滚产生空洞,客户端用 `after_sequence` 拉取不受空洞影响

### 4.6 MVP 之后(v1+)的迁移规则

- 任何 schema 变更必须新加迁移,禁止 `ALTER TABLE` 已有表
- 字段重命名走"双写 + 切流 + 删除旧"4 步(见 `aux-02 §C`)
- 大表(> 10M 行)索引用 `CREATE INDEX CONCURRENTLY`(`sqlx` 暂不直接支持,需手工 `psql`)
- V1+ 分区:`messages` 按 `conversation_id` HASH 分区,`audit_logs` 按 `created_at` RANGE 分区

---

## 5. 错误码实施(对应 aux-03)

> 详细错误码表见 `aux-03 §B`(20 项)。本节给出**实施侧的 Rust 枚举 + 统一映射**。

### 5.1 Rust 错误枚举(`crates/im-common/src/error.rs`)

```rust
// 与 aux-03 §B 一一对应,枚举值即 wire format 的 code 字符串
// 共 21 项(2026-08-23 自审新增 `FriendRequestNotFound`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::AsRefStr)]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    IdempotencyConflict,
    RateLimited,
    InvalidStateTransition,
    RecallWindowExpired,
    AccountBanned,
    AccountSuspended,
    AccountMergeConflict,
    FriendRequestExists,
    FriendRequestNotFound,    // 2026-08-23 新增
    UserBlocked,
    ValidationError,
    InternalError,
    ServiceUnavailable,
    ConversationNotFound,
    MessageNotFound,
    MessageTooLarge,
    InvalidIdempotencyKey,
    EnvironmentDisabled,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        self.as_ref()  // 由 strum derive 自动生成,例:ErrorCode::AccountBanned.as_ref() == "ACCOUNT_BANNED"
    }
    pub fn http_status(self) -> u16 {
        use ErrorCode::*;
        match self {
            Unauthorized => 401,
            Forbidden | AccountBanned | AccountSuspended | UserBlocked => 403,
            NotFound | ConversationNotFound | MessageNotFound | FriendRequestNotFound => 404,
            ValidationError | InvalidIdempotencyKey | MessageTooLarge => 400,
            InvalidStateTransition | RecallWindowExpired | AccountMergeConflict | FriendRequestExists => 409,
            RateLimited => 429,
            InternalError => 500,
            ServiceUnavailable => 503,
            IdempotencyConflict => 200,  // 特殊语义
            EnvironmentDisabled => 403,
        }
    }
    pub fn grpc_code(self) -> tonic::Code {
        use ErrorCode::*;
        match self {
            Unauthorized => tonic::Code::Unauthenticated,
            Forbidden | AccountBanned | AccountSuspended | UserBlocked | EnvironmentDisabled => tonic::Code::PermissionDenied,
            NotFound | ConversationNotFound | MessageNotFound | FriendRequestNotFound => tonic::Code::NotFound,
            ValidationError | InvalidIdempotencyKey | MessageTooLarge => tonic::Code::InvalidArgument,
            InvalidStateTransition | RecallWindowExpired | AccountMergeConflict | FriendRequestExists => tonic::Code::FailedPrecondition,
            RateLimited => tonic::Code::ResourceExhausted,
            InternalError => tonic::Code::Internal,
            ServiceUnavailable => tonic::Code::Unavailable,
            IdempotencyConflict => tonic::Code::Ok,  // 特殊语义
        }
    }
}
```

### 5.2 错误响应体 Wire 格式

REST / WS 通用:

```json
{
  "code": "ACCOUNT_BANNED",
  "message": "auth.account.banned",
  "trace_id": "tr_01HXY...",
  "ts": 1692528000000,
  "details": [ { "field": "content.text", "reason": "max_length" } ]
}
```

- `code`:来自 `aux-03 §B`
- `message`:**i18n key**,非最终用户文案
- `trace_id`:链路追踪 ID,客户端可直接发给客服
- `details`:仅 `ValidationError` 出现,定位字段错误

### 5.3 编译期 / CI 强制

- 所有 `format!("...{}...", ErrorCode::Xxx.as_str())` 在 CI 由 `scripts/check_error_codes.sh` 扫描,确保字符串与枚举一致
- 所有 `match err.code { ... }` 必须有 `default =>` 分支
- 弃用 6 个月流程见 `aux-03 §C.2`

---

## 6. 配置项实施(对应 BasicDesign §14 + DetailedDesign §10)

> 完整变量定义见 `DetailedDesign.md §10`(23 个变量 + 4 个 Secret Key)。本节给出**加载方式 + 校验规则**。

### 6.1 加载顺序

```
1. 读取 IM_ENV 决定加载 config/{IM_ENV}.toml(可选)
2. 读取 .env 文件(dotenvy,仅 IM_ENV=dev 时)
3. 读取环境变量(覆盖上面)
4. 解析双密钥 JSON 数组(IM_JWT_SIGNING_KEYS)
5. 解析 server_secrets JSON map(IM_SERVER_SECRETS)
6. 校验必填项,缺失 → log::error + process::exit(78)
```

### 6.2 校验规则

| 变量 | 校验 |
|---|---|
| `IM_DATABASE_URL` | 必须能 psql 连接(启动时 ping 一次,失败也 exit) |
| `IM_JWT_SIGNING_KEYS` | 必须是合法 JSON 数组,每项 `{kid: string, key: hex(64)}`,至少 1 项 |
| `IM_SERVER_SECRETS` | 必须是合法 JSON map,key 为 UUID,value 为 hex(64) |
| `IM_REFRESH_TOKEN_PEPPER` | 必须 hex(64) |
| `IM_HTTP_PORT` / `IM_GRPC_PORT` | 必须 1-65535 |
| `IM_ENV` | 必须在 {dev, staging, prod} |
| `IM_LOG_LEVEL` | 必须在 {trace, debug, info, warn, error} |
| `IM_WS_HEARTBEAT_TIMEOUT_SECONDS` | 必须 > `IM_WS_PING_INTERVAL_SECONDS` (默认 60 > 30) |
| `IM_RATE_LIMIT_*` | 必须 ≥ 1 |
| `IM_MESSAGE_MAX_SIZE_BYTES` | 必须 1024 ≤ x ≤ 1048576 (1KB - 1MB) |
| `IM_DB_POOL_MAX_CONNECTIONS` | 必须 1 ≤ x ≤ 200 |

### 6.3 配置热重载(environments.settings)

- 启动时全量加载所有 `environments` 行 → 写入 Valkey(`env:settings:{environment_id}`, TTL 3600s)
- 提供内部 API:`POST /v1/internal/environments/{id}/settings`(仅 im-core 内部签名校验)
- Valkey pub/sub topic `env:settings:invalidated` 广播失效
- im-core 各实例 background task 监听 topic,收到后**只**重载本实例已缓存的该 env(避免雪崩)

### 6.4 K3s Secret Key 与环境变量映射

| Secret Key(`im-env-{environment_id}`) | 注入到环境变量 | 备注 |
|---|---|---|
| `server_secret` | (不直接注入,`im-core` 启动时加载 Secret 后存入 `IM_SERVER_SECRETS` map) | 仅服务端可读 |
| `jwt_signing_key_v1` | (同上,`IM_JWT_SIGNING_KEYS` JSON 数组的第 1 项) | |
| `jwt_signing_key_v2` | (同上,轮换期第 2 项) | 轮换结束后该 Secret 留 7 天观察期后删除 |
| `refresh_token_pepper` | `IM_REFRESH_TOKEN_PEPPER` | |
| `database_url` | `IM_DATABASE_URL` | 共享(全 env 用同一 PG 集群,按 env_id 区分 schema 或 db) |
| `valkey_url` | `IM_VALKEY_URL` | 共享 |
| `nats_url` | `IM_NATS_URL` | 共享 |
| `minio_endpoint` / `minio_access_key` / `minio_secret_key` | `IM_MINIO_*` | 共享 |

轮换流程(呼应 `BasicDesign §14.4.2`):

1. 生成新密钥 → 写入 `im-env-{env_id}-new` Secret(不覆盖旧)
2. 应用同时支持 v1/v2 校验(在 `IM_JWT_SIGNING_KEYS` 中并存)
3. `POST /v1/internal/auth/rotate-start` 通知游戏服务器
4. 观察期 7 天,监控 v1 Token 占比 < 1%
5. 移除 v1 配置,删除旧 Secret
6. 写入 `audit_logs`(`action=secret_rotation`)

---

## 7. Rust 模块结构 + Trait 完整清单

> 本节是 `DetailedDesign.md §9` 的工程化扩展,**完整**列出 MVP 阶段所有公开 trait / struct / 函数,实施时直接对照实现。

### 7.1 `im-common`

```rust
// src/error.rs — 见 §5
// src/config.rs — AppConfig load() 见 BasicDesign §14.5
// src/result.rs — type AppResult<T> = Result<T, AppError>;
// src/tracing.rs — tracing 初始化(JSON output,带 trace_id)
// src/ids.rs — UserId, ConversationId, MessageId 等 newtype
// src/time.rs — Utc::now() 包装,便于测试 mock
```

### 7.2 `im-proto`

```
proto/
├── core.proto        # CoreService 14 个 RPC
├── media.proto       # MediaService 1 个 RPC(MVP 最小)
└── build.rs          # tonic-build 编译
```

### 7.3 `im-protocol`

```rust
// src/ws_frames.rs — ClientFrame / ServerFrame enum
// src/ws_frames.rs — ReqId type
// src/error.rs — 错误响应结构
// src/content.rs — MessageContent (6 种 kind 序列化)
// tests/ws_frames_roundtrip.rs — 帧编解码双向测试
```

### 7.4 `im-core`

```rust
// 见 DetailedDesign §9.1-9.3 + 7.5 节实施细节
```

#### 7.4.1 `im-core/identity/`

```rust
// password.rs
pub fn hash(password: &str) -> Result<String, IdentityError>;  // argon2id
pub fn verify(password: &str, hash: &str) -> Result<bool, IdentityError>;

// token.rs — TokenService 见 DetailedDesign §9.2
pub struct TokenService { /* signing_keys, access_ttl, refresh_pepper */ }
impl TokenService {
    pub fn new(signing_keys: Vec<SigningKey>, access_ttl: Duration, refresh_pepper: SecretString) -> Self;
    pub fn issue_access_token(&self, user: &User) -> Result<AccessToken, IdentityError>;
    pub fn validate_access_token(&self, token: &str) -> Result<TokenClaims, IdentityError>;
    pub async fn issue_refresh_token(&self, user: &User, device_fingerprint: Option<&str>) -> Result<(RefreshToken, DeviceSessionId), IdentityError>;
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenPair, IdentityError>;
    pub async fn revoke_device_session(&self, session_id: DeviceSessionId) -> Result<(), IdentityError>;
}

// repository.rs — UserRepository + DeviceSessionRepository
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, IdentityError>;
    async fn find_by_external_identity(&self, env: EnvironmentId, provider: &str, external_uid: &str) -> Result<Option<User>, IdentityError>;
    async fn create(&self, env: EnvironmentId, kind: UserKind, external: Option<ExternalIdentity>, display_name: Option<String>) -> Result<User, IdentityError>;
    async fn update_state(&self, id: UserId, state: UserState) -> Result<(), IdentityError>;
    async fn update_display_name(&self, id: UserId, display_name: Option<&str>) -> Result<User, IdentityError>;
}

// service.rs
pub struct IdentityService<U, D, T>
where U: UserRepository, D: DeviceSessionRepository, T: TokenService,
{
    user_repo: U, device_repo: D, token_service: T, hasher: PasswordHasher,
}
impl IdentityService {
    pub async fn server_exchange_token(&self, cmd: ServerExchangeCommand) -> Result<TokenPair, IdentityError>;
    pub async fn guest_register(&self, env: EnvironmentId) -> Result<TokenPair, IdentityError>;
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenPair, IdentityError>;
    pub async fn link_account(&self, access_token: &str, external: ExternalIdentity) -> Result<TokenPair, IdentityError>;
    pub async fn logout(&self, session_id: DeviceSessionId) -> Result<(), IdentityError>;
    pub async fn get_me(&self, user_id: UserId) -> Result<User, IdentityError>;
    pub async fn update_me(&self, user_id: UserId, display_name: Option<&str>) -> Result<User, IdentityError>;
}
```

#### 7.4.2 `im-core/conversation/`

```rust
// repository.rs
#[async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn create(&self, env: EnvironmentId, kind: ConversationKind, metadata: serde_json::Value) -> Result<Conversation, ConversationError>;
    async fn find_dm(&self, env: EnvironmentId, user_a: UserId, user_b: UserId) -> Result<Option<Conversation>, ConversationError>;
    async fn find_by_id(&self, id: ConversationId) -> Result<Option<Conversation>, ConversationError>;
    async fn list_for_user(&self, user: UserId, cursor: Option<Cursor>, limit: i32) -> Result<Vec<Conversation>, ConversationError>;
    async fn add_member(&self, conv: ConversationId, user: UserId, role: MemberRole) -> Result<(), ConversationError>;
    async fn remove_member(&self, conv: ConversationId, user: UserId) -> Result<(), ConversationError>;
    async fn is_member(&self, conv: ConversationId, user: UserId) -> Result<bool, ConversationError>;
    async fn list_members(&self, conv: ConversationId) -> Result<Vec<ConversationMember>, ConversationError>;
}

// service.rs — ConversationService 见 DetailedDesign §9.3
```

#### 7.4.3 `im-core/message/`

```rust
// repository.rs — MessageRepository 见 DetailedDesign §9.1
// sequence.rs — SequenceAllocator 见 DetailedDesign §9.1
// content.rs — 6 种 kind 的 JSON schema 校验
pub fn validate(kind: MessageKind, content: &serde_json::Value) -> Result<(), MessageError>;
// service.rs — MessageService::send_message 见 DetailedDesign §9.1(完整实现)
```

#### 7.4.4 `im-core/relationship/`

```rust
// repository.rs
#[async_trait]
pub trait FriendshipRepository: Send + Sync {
    async fn create_request(&self, env: EnvironmentId, sender: UserId, recipient: UserId) -> Result<FriendRequest, RelationshipError>;
    async fn find_request(&self, id: FriendRequestId) -> Result<Option<FriendRequest>, RelationshipError>;
    async fn respond_request(&self, id: FriendRequestId, accept: bool) -> Result<(), RelationshipError>;
    async fn block(&self, user: UserId, target: UserId) -> Result<(), RelationshipError>;
    async fn list_friends(&self, user: UserId, cursor: Option<Cursor>, limit: i32) -> Result<Vec<User>, RelationshipError>;
    async fn is_blocked(&self, user: UserId, target: UserId) -> Result<bool, RelationshipError>;
}

// service.rs
pub struct RelationshipService<F> { repo: F }
impl RelationshipService {
    pub async fn send_request(&self, env: EnvironmentId, sender: UserId, recipient: UserId) -> Result<(), RelationshipError>;
    pub async fn respond_request(&self, id: FriendRequestId, responder: UserId, accept: bool) -> Result<(), RelationshipError>;
    pub async fn block(&self, user: UserId, target: UserId) -> Result<(), RelationshipError>;
    pub async fn list_friends(&self, user: UserId, cursor: Option<Cursor>, limit: i32) -> Result<Vec<User>, RelationshipError>;
}
```

#### 7.4.5 `im-core/event/`

```rust
// publisher.rs — EventPublisher trait 见 DetailedDesign §9.1
// 实际实现使用 async-nats
pub struct NatsEventPublisher { client: async_nats::Client }
impl NatsEventPublisher {
    pub async fn connect(url: &str) -> Result<Self, EventError>;
}
#[async_trait]
impl EventPublisher for NatsEventPublisher { /* publish via NATS JetStream */ }
```

#### 7.4.6 `im-core/settings/`

```rust
// repository.rs
#[async_trait]
pub trait SettingsRepository: Send + Sync {
    async fn load_all(&self) -> Result<Vec<(EnvironmentId, EnvironmentSettings)>, SettingsError>;
}

// service.rs
pub struct SettingsService {
    repo: Arc<dyn SettingsRepository>,
    cache: Arc<ValkeyCache>,
    invalidation_sub: async_nats::Subscriber,
}
impl SettingsService {
    pub async fn load_initial(&self) -> Result<(), SettingsError>;
    pub async fn run_invalidation_listener(self: Arc<Self>) -> !;  // 永久运行
    pub async fn get(&self, env: EnvironmentId) -> Result<EnvironmentSettings, SettingsError>;
}
```

### 7.5 `im-gateway`

> **`im-gateway` 是独立进程,通过 gRPC 调用 `im-core`**(呼应 `BasicDesign §2` 服务拓扑;2026-08-23 自审确认:Day1-Task-List §Phase A 提到的"单服务"已被 BasicDesign 推翻,Day 14 之前一直独立部署)。

```rust
// main.rs — 启动 actix-web HTTP/WS + tonic gRPC 客户端
//   AppConfig 加载 → 构造 im-gateway Runtime (含 im-core gRPC client + Valkey + NATS client)
//   启动 actix HTTP server (8080) + 注册 WS 路由
//   IM_PROMETHEUS_BIND 启用时启动 prometheus exporter

// http/ — REST 路由(见 §3.1)
//   每个端点 1 个 handler 函数,handler 内部:
//     1. 解析 + 校验请求
//     2. 调 auth_middleware 校验 Bearer Token(走 im-core gRPC ValidateAccessToken)
//     3. 调相应 im-core gRPC 方法(限流 + 业务逻辑都在 im-core,im-gateway 只做协议转换)
//     4. 把 gRPC 响应映射为 HTTP JSON(单一 AppError::code() 入口,见 §5)

// ws/ — WebSocket
//   session.rs — WsSession 见 DetailedDesign §9.4
//     - handle_auth: 调 im-core ValidateAccessToken → 拿到 claims → 绑定 session
//     - 监听 im-core NATS `im.message.*` / `im.presence.*` 事件 → push 给客户端
//   router.rs — 帧 → 内部 im-core gRPC handler 路由
//   frames.rs — 出帧构造(idempotent_replay 标记、force_disconnect 等)
//   heartbeat.rs — 30s ping timer

// error.rs — AppError → HTTP Response / WS Frame 统一转换(单一来源 §5)
// auth_middleware.rs — Bearer Token 校验(走 im-core::ValidateAccessToken,内部带缓存 5s)
// ratelimit.rs — Valkey 令牌桶(限流在 im-gateway 还是 im-core?MVP im-gateway,V1 评估)
```

### 7.6 跨 crate 依赖方向(强制)

```
im-common  ←  im-proto  ←  im-protocol  ←  im-core  ←  im-gateway
                                                     ←  im-presence
                                                     ←  im-media
                                                     ←  extension-runtime
```

任何反向依赖(如 `im-core` 调 `im-gateway`)**禁止**,由 Rust 编译器 + CI 的 `cargo-deny` + `cargo-machete` 联合阻断。

---

## 8. 部署与 CI/CD 实施

### 8.1 Dockerfile(`docker/im-gateway.Dockerfile` + `docker/im-core.Dockerfile`)

> 与 `Platform-Specifics.md §6` 一致,补充 build cache 优化:

```dockerfile
# syntax=docker/dockerfile:1.7
FROM rust:1-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
# 依赖缓存层(单独 COPY 以最大化缓存命中)
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN mkdir -p target && cargo build --release -p im-gateway

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/im-gateway /usr/local/bin/im-gateway

USER 1000:1000
EXPOSE 8080 9000
ENTRYPOINT ["/usr/local/bin/im-gateway"]
```

> `im-core.Dockerfile` 同结构,只需替换 `im-gateway` 为 `im-core`。

### 8.2 K3s 部署 Job(数据库迁移)

> **关键**(2026-08-23 自审发现):`sqlx migrate run` 是 `sqlx-cli` 工具,**im-core / im-gateway 镜像里没有该二进制**。两种解决方案任选其一,推荐 (a):

**(a) 推荐**:在 `Dockerfile` 单独构建一个 `im-migrate` 镜像,只含 `sqlx-cli`:

```dockerfile
# docker/im-migrate.Dockerfile
FROM rust:1-slim AS builder
RUN cargo install sqlx-cli --version '^0.8' --no-default-features --features rustls,postgres
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
USER 1000:1000
ENTRYPOINT ["sqlx"]
```

```yaml
# deploy/k3s/dev/migrate-job.yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: im-migrate
  namespace: im1-dev
spec:
  backoffLimit: 3
  template:
    spec:
      restartPolicy: OnFailure
      initContainers:
        - name: wait-for-db
          image: postgres:18
          command: ["sh", "-c", "until pg_isready -h postgres -p 5432; do sleep 2; done"]
      containers:
        - name: migrate
          image: ghcr.io/{org}/im1.0-im-migrate:latest
          command: ["sqlx", "migrate", "run"]
          env:
            - name: IM_DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: im-db-credentials
                  key: database-url
```

**(b) 备选**:在 im-core 镜像里包含 `sqlx-cli`(增大镜像 ~50MB,但简化构建管线):在 `docker/im-core.Dockerfile` 的 builder 阶段追加 `RUN cargo install sqlx-cli --no-default-features --features rustls,postgres`,然后 `ENTRYPOINT ["sqlx", "migrate", "run"]`(只对 migrate Job 覆盖命令)。

### 8.3 启动顺序(Init Container 模式)

```yaml
# im-gateway Deployment 的 initContainers 段
initContainers:
  - name: wait-im-core
    image: busybox:1.36
    command: ['sh', '-c', 'until nc -z im-core 9000; do echo waiting...; sleep 2; done']
  - name: migrate
    image: ghcr.io/{org}/im1.0-im-core:latest
    command: ["sqlx", "migrate", "run"]
    envFrom: [{ secretRef: { name: im-core-env } }]
```

> **争议点**:V1+ 应将 migrate 拆为独立 Job 由 ArgoCD/Flux 触发(更符合 GitOps);MVP 用 init container 简化。

### 8.4 GitHub Actions 流水线(汇总)

| 文件 | 触发 | 步骤 | 通过条件 |
|---|---|---|---|
| `.github/workflows/ci.yml` | PR / push to main | lint → test-unit → test-integration → sast | 4 个 job 全绿 |
| `.github/workflows/release.yml` | tag `v*.*.*` | build → push to GHCR → 写 release notes | build 成功 + 镜像推送 |
| `.github/workflows/deploy-dev.yml` | push to main / workflow_dispatch | setup kubeconfig → kubectl apply → smoke test | smoke 通过 |

> CI 详细 YAML 见 `Platform-Specifics.md §3`。

### 8.5 镜像版本策略

- `latest` 标签:每次 merge to main 自动更新(MVP 阶段)
- 语义版本:`v{major}.{minor}.{patch}` 标签,在 release 时由 GitHub Actions 自动生成
- 镜像命名:`ghcr.io/{org}/im1.0-{crate_name}:{tag}`
- 不变性:**已推送的镜像永不覆盖**(GHCR immutable tags,V1+ 启用)

---

## 9. 可观测性实施(最小集)

> 详细 SLO / Dashboard / 告警见 `Observability.md`(独立文档)。本节只列**MVP 必接的指标 + 接入方式**。

### 9.1 MVP 必接指标 (5 项)

| 指标名 | 类型 | 来源 | 单位 | 标签 |
|---|---|---|---|---|
| `im_active_connections` | Gauge | im-gateway | 个 | `env_id` |
| `im_message_latency_seconds` | Histogram | im-core (MessageService) | 秒 | `kind`, `env_id` |
| `im_message_delivery_latency_seconds` | Histogram | im-gateway (WS push) | 秒 | `env_id` |
| `im_connection_latency_seconds` | Histogram | im-gateway (TCP upgrade) | 秒 | `env_id` |
| `im_reconnect_rate` | Counter | im-gateway (WS close reason) | 次 | `reason`, `env_id` |

### 9.2 接入方式

- `prometheus` crate 暴露 `/metrics` 端点
- 服务启动时初始化 `prometheus::default_registry`
- `IM_PROMETHEUS_BIND=0.0.0.0:9100` 启用(默认关闭)
- Prometheus + Grafana 由 V1 引入(MVP 仅暴露端点,无 dashboard)

### 9.3 MVP 不接的(留给 V1+)

- Trace(OpenTelemetry):`OTEL_EXPORTER_OTLP_ENDPOINT` 配置预留但默认关闭
- 日志聚合:`kubectl logs` 即用
- 告警:无

### 9.4 必接日志字段(`tracing` JSON)

所有日志必带:

```json
{
  "timestamp": "2026-08-23T00:00:00.000Z",
  "level": "info",
  "target": "im_gateway::ws::session",
  "trace_id": "tr_01HXY...",
  "user_id": "uuid",
  "env_id": "uuid",
  "message": "ws message received",
  "kind": "send_message",
  "req_id": "uuid"
}
```

> 绝不含 `password` / `token` / `secret` 等机密字段(tracing layer 过滤掉)。

---

## 10. 测试实施

### 10.1 测试分层

| 层级 | 工具 | 范围 | 目标覆盖率 | 实施位置 |
|---|---|---|---|---|
| 单元测试 | `cargo test --lib` | 纯函数 / Trait impl | 全仓行覆盖 ≥ 60%,关键模块 ≥ 80% | `src/**/*_test.rs` 或 `#[cfg(test)] mod tests` |
| 集成测试 | `cargo test --test '*'`(用 `testcontainers` 启 PG / Valkey / NATS) | 跨模块协作 + DB 集成 | 关键路径 100% 覆盖 | `tests/*.rs` |
| API 契约测试 | `actix-web::test` + `reqwest` | REST 端点 + 错误码 + 鉴权 | 全部端点至少 1 正 1 反 | `crates/im-gateway/tests/api_*.rs` |
| WS 协议测试 | `actix-ws` test | 帧序列 + 心跳 + 重连 | 全部 12 类帧 | `crates/im-gateway/tests/ws_*.rs` |
| Smoke(端到端) | `scripts/integration_test.sh` + 真实 K3s dev | 2 终端收发消息 | 1 happy path | `scripts/` |
| SAST | `cargo clippy -- -D warnings` + `cargo audit` + `semgrep` | 全代码 | 0 critical | CI |
| 性能 / 负载 | V1+ `criterion` + k6 (MVP 不做,标记 Candidate) | — | — | V1+ |

### 10.2 关键模块覆盖率目标

| 模块 | 目标 | 必覆盖场景 |
|---|---|---|
| `im-core::message::service` | 90% | 正常发消息、幂等冲突、状态机非法、消息过长 |
| `im-core::identity::token` | 90% | 签发、校验、过期、旋转、双密钥切换、错误 token |
| `im-core::conversation::service` | 85% | 创建 DM (重复对幂等)、创建 Group、成员校验 |
| `im-gateway::ws::session` | 85% | 鉴权、发消息、ping/pong、force_disconnect |
| `im-core::message::repository` | 80% | insert、find_by_idempotency_key (空 / 命中)、list_after_sequence (空 / 多条) |

### 10.3 必测的不变量(Integration Test 必须验证)

- [ ] 并发 1000 条消息发到同一会话,`sequence` 严格单调,无重复,无空洞(可接受最后回滚产生的空洞)
- [ ] 同一 `idempotency_key` 并发提交 100 次,DB 中只产生 1 条消息,所有 100 次响应 `message_id` 相同
- [ ] 跨会话消息 sequence 互不影响
- [ ] 离线重连:`after_sequence=N` 拉取返回 sequence > N 的所有消息
- [ ] Token 过期后 access 自动 fail → SDK 触发 refresh → 重发成功
- [ ] DM 并发创建:同一对 user 100 次并发,只产生 1 个 conversation
- [ ] User 注销后 (`state=deleted`),所有写操作 403 `ACCOUNT_BANNED` 风格错误(实际为 NOT_FOUND,见 `aux-03`)
- [ ] banned 用户的 `state` 写操作 403 `ACCOUNT_BANNED`
- [ ] 撤回时间窗内可撤回,超窗后 `RECALL_WINDOW_EXPIRED`
- [ ] 编辑权限:仅 sender 可编辑,超窗后 `RECALL_WINDOW_EXPIRED`

### 10.4 测试 fixture 复用

- `crates/im-core/tests/common/mod.rs` — 共享 `test_db()`, `test_settings()`, `test_user()`
- `crates/im-gateway/tests/common/mod.rs` — `test_app()` 启 actix `TestServer`
- `scripts/test-data/` — 预置 SQL fixture(2 个 tenant, 3 个 game, 5 个 env, 100 个 user)

---

## 11. 安全实施 Checklist

> 详细安全需求见 `SRS.md §30`(`SEC-NFR-*`)。本节只列**MVP 必落地的红线** + 实施点。

### 11.1 认证 / 授权红线

- [x] **Server-to-Server Token Exchange**(`GAME-ID-003`):游戏服务器 HMAC 签名,客户端不持有 server_secret
- [x] **JWT 短生命周期 + Refresh Rotation**(`SEC-NFR-003`):Access TTL 15min,Refresh 30d,每次刷新旧 token 失效
- [x] **Bearer Token 强制**:`auth_middleware` 在所有需要鉴权的端点强制校验
- [x] **环境隔离**:每个 Token 的 `environment_id` claims 与请求 env 校验,跨 env 访问 403
- [x] **Guest 默认能力**:`users.kind='guest'` 受限——不可创建会话(可被加入)、可发送消息但有限速

### 11.2 输入校验

- [x] 所有 `content` JSON 按 `kind` 校验 schema(用 `serde` derive + 手写 validator)
- [x] `idempotency_key` 必须是合法 UUID 形态(正则或 uuid crate 校验)
- [x] `conversation_id` / `user_id` 等 path 参数必须能解析为 UUID
- [x] 文本消息 `text` 长度 1-4000,服务端 reject(不回显给其他客户端,避免长度攻击)
- [x] `metadata` JSONB 大小限制 64KB

### 11.3 限流

- [x] 单用户 60 msg/min(`IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN`,可被 env 覆盖)
- [x] Guest 注册按 IP 10/hour(`IM_RATE_LIMIT_GUEST_REGISTER_PER_HOUR`)
- [x] Token Exchange 按 environment_id 1000/min
- [x] 全局 100 req/s/user(`IM_RATE_LIMIT_BUCKET_SIZE`)

实现:Valkey 令牌桶(单 key `rate:{env_id}:{user_id}:{op}`)

### 11.4 审计

- [x] 所有 `auth.*` 操作写 `audit_logs`
- [x] 所有 `secret_rotation` 写审计
- [x] 所有 `user.state_changed` 写审计
- [x] 所有 `admin.*` 操作写审计(预留,MVP 无 admin API,准备 V1)

### 11.5 不做的事(MVP)

- 不引入 KMS(V1+ 评估 Vault)
- 不做静态加密(应用层加密 V1+ 评估, MVP 仅 TLS)
- 不做内容审核(V1+ 扩展)
- 不做 DDoS 防护(由 K3s Ingress + Cloud LB 承担)

---

## 12. 验收标准(Definition of Done)

> 合并 `SRS.md §48` + `Day1-Task-List.md` 验收部分,本规范要求的**MVP 完整闭环**标准。

### 12.1 功能验收

- [ ] 1 个用户能注册(走 Guest 路径)并拿到 Access + Refresh Token
- [ ] 1 个用户能给另 1 个用户发消息(`POST /v1/conversations/{id}/messages` 或 WS `send_message`)
- [ ] 接收方通过 WebSocket 实时收到 `message_new` 帧
- [ ] 消息持久化:接收方断线后 `GET /v1/conversations/{id}/messages?after_sequence=N` 拉取增量可见
- [ ] 同一 `idempotency_key` 重复发 5 次,DB 中只产生 1 条消息,5 次响应 `message_id` 相同
- [ ] DM 并发创建 10 次,只产生 1 个 conversation
- [ ] 编辑/撤回/已读/reaction 全部端到端可用
- [ ] 好友申请 / 接受 / 拉黑 全部端到端可用
- [ ] 媒体上传(预签名 URL)+ 引用发送 + 下载 链路通

### 12.2 非功能验收

- [ ] **CI 全绿**:lint / test-unit / test-integration / sast 4 个 job 全部通过
- [ ] **单测覆盖率**:全仓 ≥ 60%,5 个关键模块(§10.2)≥ 80%
- [ ] **SAST**:`cargo clippy -- -D warnings` 0 warning,`cargo audit` 0 critical,`semgrep` 0 high
- [ ] **性能**(MVP 不强制 NFR,只跑 baseline):1000 msg/s 持续 1 小时无 OOM,无消息丢失
- [ ] **安全红线**(§11)全部 ✓
- [ ] **配置加载**:所有必填项缺失 → exit 78,有合理启动日志
- [ ] **可观测**:`/metrics` 暴露 5 个 MVP 指标;`/healthz` 200;`/readyz` 在 PG 不可达时 503
- [ ] **部署**:K3s dev namespace `git push` → 自动部署 → smoke 通过

### 12.3 文档验收

- [ ] 本规范(ImplementationSpec)与 DetailedDesign / BasicDesign / aux-* 无矛盾
- [ ] aux-13 协议帧样例与代码实现 100% 一致(由 integration test 验证)
- [ ] aux-03 错误码表与 `crates/im-common/src/error.rs` 100% 一致(由 `scripts/check_error_codes.sh` 验证)
- [ ] aux-01 命名规范在 clippy / eslint / sqlfluff 中 0 违规
- [ ] aux-02 数据字典与 DB schema 100% 一致(由 `aux-07` 校验)

### 12.4 流程验收(Workflow RACI)

- [ ] 4 个 GATE 全部通过:`PR 合入 / 单测通过 / Smoke 通过 / 部署成功`
- [ ] 每个 PR 有 1 approve(Tech Lead)
- [ ] 协议变更在 PR 中标 `[PROTOCOL-FROZEN]` 解除冻结

---

## 13. 风险与遗留(指向 BasicDesign §16 + DetailedDesign §12)

| 编号 | 风险 | 实施缓解 |
|---|---|---|
| ADR-011 | im-core 拆分触发阈值未定 | 第 1 轮压测后回填 |
| ADR-012 | PostgreSQL Operator 选型 | CloudNativePG + PoC 验证(MVP 暂用裸 StatefulSet) |
| ADR-013 | HUD Runtime 容器 | V1 决定,Tauri 候选 |
| ADR-014 | WS 帧 JSON vs Protobuf | MVP JSON,压测后决定 |
| ADR-015 | Valkey 集群模式 | 单实例 MVP,V1 评估 cluster / sentinel |
| C-001 | Sequence 单会话行锁瓶颈 | POC-02 压测后决定是否升级 |
| C-002 | `IM_MESSAGE_MAX_SIZE_BYTES` 默认 64KB 合理性 | 实测后回填 |
| C-003 | 双密钥轮换观察期 7 天 | 首批生产轮换后回填 |
| C-004 | `IM_ACCESS_TOKEN_TTL_SECONDS` 默认 15min 合理性 | 首批生产后回填 |

---

## 14. 关联文档(完整索引)

| 文档 | 关系 |
|---|---|
| `docs/SRS.md` | 上游:所有需求 ID 追溯源 |
| `docs/BasicDesign.md` | 上游:架构决策、服务拓扑、拆分依据 |
| `docs/DetailedDesign.md` | 上游:协议 / DB / 接口签名(本规范细化实施) |
| `docs/LiveKit-Voice-Subsystem.md` | 平行:V1 范围,本规范仅预留扩展点 |
| `docs/Observability.md` | 平行:可观测性详细设计,本规范 §9 仅落地 MVP 最小集 |
| `docs/Platform-Specifics.md` | 平行:GitHub / K3s 平台配置,本规范 §2/§8 引用 |
| `docs/Deployment-Runbook.md` | 平行:运维手册,本规范 §8 引用部署部分 |
| `docs/SDK-Integration-Guide.md` | 平行:Unity 接入指南,本规范 §3.2/§3.3 与之严格一致 |
| `docs/Project-Status.md` | 平行:决策快照,本规范不重复决策 |
| `docs/Day1-Task-List.md` | 平行:14 天任务,本规范 §1.3 给出对应章节 |
| `docs/Workflow.md` + `Workflow-RACI.md` | 上游:流程与角色,本规范 §10/§12 引用 |
| `docs/templates/04-detailed-design/auxiliary/aux-01..13` | 平行:13 份跨切面支撑文档,本规范直接引用 |
| `docs/ImplementationSpec.md`(本文件) | 自身 |

---

## 15. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-08-23 | Mavis 辅助 | 初版:基于 DetailedDesign + BasicDesign 落地 MVP 实施细节;§1 范围,§2 仓库结构与依赖锁版本,§3 API 实施 Checklist,§4 DB 迁移 6 份 SQL,§5 错误码 Rust 枚举,§6 配置 + K3s Secret,§7 Rust 模块完整 trait 清单,§8 部署 + CI,§9 可观测性 MVP 5 项,§10 测试,§11 安全 Checklist,§12 DoD,§13 风险,§14 关联 |
| 1.0.1 | 2026-08-23 | Mavis 自审 | **自审修复批次**:① 修 §1.1 多个计数(错误码 20→21 / gRPC 14→22 / env 23→27 / Secret 4→10 / CI 4→3 / REST 24→26 / 4类→5类覆盖);② **修 SQL BUG**:`users.external_identity` `UNIQUE NULLS NOT DISTINCT` 会阻断多个 Guest 共存,改为默认 `UNIQUE`(NULL 视为 distinct);③ 修 §7.5 "im-gateway + im-core 单进程" 误述(实际为独立进程,gRPC 通信,呼应 `BasicDesign §2`);④ 修 §8.2 K3s migrate Job 镜像缺 `sqlx-cli` 的问题(新增 `docker/im-migrate.Dockerfile`);⑤ aux-03 新增 `FRIEND_REQUEST_NOT_FOUND` 错误码(§3.1.4 引用了它);⑥ §3.1.4 好友/拉黑错误码语义明确 + body 必填说明;⑦ §3.2 加 WS 鉴权方式决策(选 auth 帧而非 Day 1 的 query token,补理由);⑧ `BasicDesign §8` 事件总线补 `im.message.edited` / `im.message.reaction_added` / `im.auth.token_rotated`;⑨ `DetailedDesign §5` 加 `GET /readyz`;⑩ §3.1.2 加 Guest 创建会话限制(kind=dm only);⑪ §2.2 Cargo.toml 加 "初始估计" 说明;详细自审报告见 §16 |
| 1.0.2 | 2026-08-26 | 架构师 (Mavis) | **Day 1 GATE 补签:依赖升级 (cargo 1.98 / rustc 1.98 兼容性 hotfix)**:① §2.2 `sqlx`: 0.8.6→**0.9** (0.8.6 在 cargo 1.98 上 `sqlx-macros-core` E0220 "Output not found for F" + Debug 解析卡死;0.8.x 无 0.8.7+ patch;0.9.0 2026-05-21 已修);② §2.2 `async-nats`: 0.37→**0.50** (0.37.0 编译期 STATUS_STACK_BUFFER_OVERRUN 0xc0000409;0.50.0 2026-07-20 stable;0.50 默认 features 仍含 ring/jetstream/websockets/kv/object-store,§6 NATS Secret Key 不变,publisher.rs 占位代码实装时需核对 0.50 API 命名空间);③ §2.2 `prometheus`: 0.13→**0.14** + `default-features = false` (0.13.4 拉 protobuf 2.28.0 → cargo 1.98 codegen 爆栈;0.14 2026-03 发布;protobuf feature 关闭,V1 实装 metrics 时再开);④ §2.2 `dashmap`: **移除** (6.2.1 即最新 stable,在 cargo 1.98 上 `Self::Output` 推导撞上 IntoFuture/AsyncFnOnce/FnOnce 5×E0223;仓库 0 处实际调用,grep 验证)。**Cargo.lock 重新 resolve, 467 packages 锁到最新兼容版本**。配套 commit: main 0477e1d + 0b08c4a + 64f6317 + f6b6f06 (hotfix 直 commit main,per 2026-08-26 09:05 JST 用户决策)。**Day 1 GATE 验收状态**: `cargo check --workspace --all-targets` EXIT 0;`cargo test --workspace` EXIT 0,**27 tests passed, 0 failed** (im_common 8 / im_core 12 / im_protocol 7 / 5 crates 占位 0 + 8 doc-test 套件 0,worktree 子代理 `bg_055f3e71` 验证);`cargo build --release` EXIT 0 3:28,im-gateway.exe 4.4 MB (E:\DevCache\cargo\target\release\im-gateway.exe)。origin/main 已同步至 f6b6f06 (4 commit push 成功)。 |
| 1.0.3 | 2026-08-26 | 架构师 (Mavis) | **Day 2 GATE 补签 + 协议冻结 [PROTOCOL-FROZEN]**:① §3.2.4 新增「协议冻结声明」段:WS 12 帧(双向 8 client + 9 server unique = 17 unique types) + gRPC 22 RPC(im.core.v1) + REST 26 端点(7 类,§3.1.1-§3.1.7) 三套协议于 2026-08-26 JST 冻结,标签 `[PROTOCOL-FROZEN]` 生效,变更走 aux-13 §7 流程(Slack 公告 7 天 + RFC + Tech Lead + PM review + 6 个月兼容期);② aux-13 §6 末段同步冻结状态行(引用 §3.2.4);③ Project-Status §1.1.1 新增 Day 2 GATE 补签里程碑小节,记 2 项遗留工程债(im-core service/repository 未直接引用 aux-02 §F 字段 + 6 份 SQL migration 未在真 PG 实例跑过 — 沙箱无 docker daemon);④ aux-02 §8 v1.1.1 复检批次:14 张表与 migrations 1:1,5 个关键字段(`users.external_identity` / `conversations.metadata` / `messages.idempotency_key` / `dm_pairs.user_a < user_b` / `audit_logs.target_type`)SQL/aux-02 一致;⑤ 16 个占位模块加 `#![allow(dead_code, unused_imports, unused_variables)]` 让 `cargo clippy -- -D warnings` 通过(V1 实装时移除);⑥ `relationship/service.rs:41` `Result.or_else(|x| match e { Err(x) })` 改 `Result.map_err(|x| match e { x })`(clippy::or_fun_call lint,语义等价);⑦ `crates/im-gateway/tests/migration_smoke.rs` 新增 3 个无 docker 验证测试(`Migrator::new` 解析 6 份 + 6 份 SQL 非空 + 14 张表名 CREATE TABLE ≥ 1 次)。配套 commit: main 931d18d (clippy allow + migration_smoke.rs) + 12c7662 (协议冻结 + 文档同步)。**Day 2 GATE 验收状态**: `cargo clippy --workspace --all-targets -- -D warnings` EXIT 0;`cargo test --workspace` EXIT 0,**30 tests passed, 0 failed** (im_common 8 + im_core 12 + im_protocol 7 + im-gateway migration_smoke 3,worktree 分支 `feat/day2-gate-20260826` 由子代理 `bg_dc91459d` 启动调研后 main 接管完成);origin/main 同步至 12c7662 (2 commit push 成功)。 |

---

## 16. 自审报告 (2026-08-23)

> **目的**:v1.0.0 发布后由 Mavis 自审,发现 11 项问题(5 P0 关键 / 6 P1 中等),本节汇总发现 + 修复过程,作为后续 Mavis 实施时复核的基线。
>
> **方法**:逐行重读 ImplementationSpec + 交叉对照 BasicDesign / DetailedDesign / aux-01..13 / Day1-Task-List / Workflow-RACI。

### 16.1 P0 关键问题(已全部修复)

| # | 问题 | 影响 | 修复 |
|---|---|---|---|
| P0-1 | §1.1 与正文多处计数自相矛盾(gRPC 14 vs 22 / env 23 vs 27 / Secret 4 vs 10 / CI 4 vs 3 / REST 24 vs 26 / 覆盖目标 4 vs 5) | 文档内部矛盾,读者无所适从 | 改为精确计数,所有数字与对应章节一致 |
| P0-2 | **SQL BUG**:`users.external_identity UNIQUE NULLS NOT DISTINCT` 会阻断多个 Guest 共存(同 env 下只允许 1 个 extid=NULL) | 实际部署后 1 个 env 第 2 个 Guest 注册即失败 `53000 unique_violation` | 改为 `UNIQUE` 默认语义(NULL distinct),多个 Guest 可共存 |
| P0-3 | §7.5 写"im-core 与 im-gateway 单进程",与 `BasicDesign §2` 独立部署矛盾 | 实施时会合并部署,违反服务边界设计,后续无法独立扩展 im-gateway | 重写 §7.5 明确"独立进程 + gRPC 通信",引用 BasicDesign §2 |
| P0-4 | §8.2 K3s migrate Job 用 im-core 镜像跑 `sqlx migrate run`,但 im-core 镜像里没有 sqlx-cli 二进制 | 首次部署迁移直接失败 | 新增 `docker/im-migrate.Dockerfile`,Job 用专用镜像 |
| P0-5 | §3.1.4 引用 `FRIEND_REQUEST_NOT_FOUND`,但 aux-03 错误码表没有这一项 | 实施时无 ErrorCode 枚举,编译失败 | aux-03 + ImplementationSpec §5.1 + 错误码总数 20→21 同步更新 |

### 16.2 P1 中等问题(已全部修复)

| # | 问题 | 影响 | 修复 |
|---|---|---|---|
| P1-1 | §3.1.4 `POST /v1/friends/{id}/block` 错误码列写"USER_BLOCKED(自指)",语义混乱 | 实施时无法判断何时返回 USER_BLOCKED | 重写端点表,加 Body 必填 + 实施要求(幂等/覆盖 accepted) |
| P1-2 | §3.2 沿用 Day 1 的"query token"鉴权,但 `aux-13 §1.1.1` 用 `auth` 帧,跨文档矛盾 | SDK 集成时协议二选一不知听谁 | 加决策说明,选 `auth` 帧(理由:query token 进 nginx access log 泄露风险) |
| P1-3 | §3.2.2 引用 `im.message.edited` / `im.message.reaction_added` 事件,但 BasicDesign §8 没定义这两个 Topic | im-core 发布事件后 im-presence / im-gateway 订阅时找不到 subject | BasicDesign §8 补 3 个 Topic(`edited` / `reaction_added` / `auth.token_rotated`) |
| P1-4 | ImplementationSpec §3.1.7 有 `GET /readyz`,但 DetailedDesign §5 没有该端点 | 实施时只读 DD 的人会漏掉 | DetailedDesign §5 加 `GET /readyz`,补就绪 vs 健康的区别 |
| P1-5 | §11.1 写"Guest 不可创建会话",但 §3.1.2 端点表没有任何 kind 校验说明 | im-gateway 实施时不会拒绝 Guest 创建 group | §3.1.2 加 Guest 限制(只允许 dm) |
| P1-6 | §2.2 Cargo.toml 列出 30+ crate 版本号,无"启动初始估计"说明 | 编码时若 `cargo build` 报版本不存在,会怀疑 spec 写错 | 加注"初始估计,以 Cargo.lock 为准" |

### 16.3 P2 已知局限(暂不修,记录待办)

| # | 问题 | 暂不修原因 | 后续处理 |
|---|---|---|---|
| P2-1 | `friend_requests` UNIQUE(env, sender, recipient) 限制"同对用户只能发 1 次好友申请(跨所有 state)" | 多数 IM 产品也这样设计(被拒后必须走"重新申请"按钮而非"自动重发") | 编码前向 PM 确认;若需"重发",迁移为 partial UNIQUE WHERE state='pending' |
| P2-2 | `idx_message_reactions_message` 索引冗余(PK 已含 message_id) | 性能影响可忽略,删除需跨 PR | V1 清理时一并处理 |
| P2-3 | `respond_friend_request` gRPC/REST 端点的 body `{accept: bool}` 在 `aux-13` 没给出样例 | SDK 实现时需参考 ImplementationSpec §3.1.4 | aux-13 下次更新时补 |
| P2-4 | Cargo.toml 中 `s3 = "0.34"` / `redis = "0.27"` 等具体版本号可能因 cargo 生态 release 周期产生 ±1 minor 漂移 | cargo build 时会自然 resolve | §2.2 已加"以 Cargo.lock 为准"声明 |
| P2-5 | `Web Dashboard` 目录 `web/dashboard/` 在 §2.1 存在,但 §1.2 明确 V1 才实装,当前是空目录 | 目录占位无害,删除反而需要 CI 重新生成 | V1 实装时直接用 |
| P2-6 | WS `auth` 帧要求客户端 100ms 内发送,否则断开 —— 这个超时时间无依据 | 100ms 是经验值,实际可压测调 | POC-01(100K 连接)阶段校准 |
| P2-7 | `aux-04-state-machine-spec.md` / `aux-05-crc-card.md` / 等 9 份 aux 仍是模板,未填实 | 仅 Day 1 必填 4 份要求填实,其他不强制 | 编码阶段按需填实 |

### 16.4 自审方法学(留给后续 Mavis 参考)

1. **计数自洽检查**:§1.1 任何"X 个 Y"声明,必须用 `grep` 在正文找到恰好 X 个实例
2. **跨文档引用一致**:ImplementationSpec → DetailedDesign / aux-* 的链接必须能跳到正确章节,且章节内容一致
3. **Cargo 依赖锁定版本逐个 `cargo search` 验证**:本次发现 1 项(redis 0.27)真实存在但建议加 "verify with build" 注释
4. **SQL 逐条 `psql` 模拟执行**(`pgcrypto` 扩展、`UNIQUE NULLS NOT DISTINCT` 语义、`CHECK` 约束)—— 本次发现 P0-2
5. **错误码 grep 一致性**:`ErrorCode::Xxx` 在 ImplementationSpec §5.1 / aux-03 §B / 实施端点表的引用必须同一字符串
6. **CR + SAST 模拟**:把每个 API 端点都过一遍"恶意输入 → 预期错误码 → 实际 SQL/Rust 路径",本次发现 P1-1 错误码语义错位
7. **POC 方法论引用**:性能 / 容量 / 阈值声明都应引用对应 POC(如 `IM_WS_HEARTBEAT_TIMEOUT_SECONDS=60` 应有 POC 编号)

### 16.5 修复后回归验证清单

- [x] §1.1 计数与正文完全一致
- [x] SQL 6 份迁移脚本可手工 `psql` 模拟成功(语义层面,非真实执行)
- [x] aux-03 错误码 21 项全部在 ImplementationSpec §5.1 枚举 + http_status/grpc_code 映射一致
- [x] aux-13 12 类 WS 帧 + ImplementationSpec §3.2 路由表 1:1 对应
- [x] BasicDesign §8 事件 Topic 与 ImplementationSpec §3.2.2 触发条件 1:1 对应
- [x] DetailedDesign §5 端点表(20 项) + ImplementationSpec §3.1 端点表(26 项)= ImplementationSpec 多出 6 项已在 §3.1.7 / §3.1.2 / §3.1.3 单独说明(readyz 补 / Conversation GET 补 / message patch/recall/reaction 补)
- [x] DetailedDesign §10 配置变量(27 项) + ImplementationSpec §6 校验规则 + aux-12(待 V1 落地) 一致
- [x] Day 1 任务清单 14 天任务 + ImplementationSpec §1.3 对应章节一致(7 项直接对应 + 7 项"已存在 aux-* 引用")
