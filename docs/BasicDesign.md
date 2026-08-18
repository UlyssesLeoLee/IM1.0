# 可嵌入式游戏 IM 平台 基本设计书（Basic Design）

版本：v0.1 Draft ｜ 范围：**MVP**（对应 `docs/SRS.md` 第45章 MVP Scope）
上游依据：`docs/SRS.md`（产品/需求）、`docs/LiveKit-Voice-Subsystem.md`（语音，V1起纳入，本版本仅预留接口边界）
技术栈基线（不可变更）：Rust（主）+ Python（仅AI扩展，MVP不启用）+ Next.js（Web Dashboard）+ K3s + PostgreSQL + Valkey + NATS JetStream + MinIO，全部依赖须开源可商用。

本文档面向详细设计与编码阶段，粒度到：服务边界、进程内模块、数据表结构、API 契约草案、消息队列 Topic、部署拓扑、配置项。不包含语音子系统的详细设计（见 LiveKit 文档，其详细设计另行输出）。

---

## 1. 设计范围与非目标

**本版本设计的是 MVP**（SRS 第45章）：Identity(User/Guest/Device/Token)、Relationship(好友基础模型)、Conversation(DM/Group/Channel/System)、Message(文本/图片/回复/mention/编辑/删除/撤回)、Presence(在线状态)、Game SDK 五步接入(Unity优先)、Game Identity Bridging(Guest+自定义JWT)、Laser HUD、基础安全(Token/RBAC/ABAC/审计)、Extension Runtime **骨架**(不含任何具体AI/Work扩展实现)。

**不在本版本**：语音（LiveKit集成，V1）、AI Extension、Work Extension、Guild/Party等游戏社交映射的完整实现（仅预留metadata扩展点）、多区域部署、Vector/Graph能力。

## 2. 服务拓扑（对应 SRS 第39章逻辑边界）

```
                         ┌─────────────────────────┐
                         │   Next.js Web Dashboard  │  （管理台/Full Client Web版）
                         └────────────┬─────────────┘
                                      │ HTTPS (REST/GraphQL 待定，MVP用REST)
                                      ▼
┌──────────────────────────────────────────────────────────────────────┐
│                              im-gateway (Rust)                        │
│  职责：WebSocket长连接终结、HTTP API入口、鉴权前置校验、Rate Limit、    │
│         协议转换（WS帧 ⇄ 内部gRPC）、连接会话状态（本地+Valkey）        │
└───────────┬───────────────────────────────┬───────────────────────────┘
            │ gRPC (tonic)                  │ gRPC
            ▼                               ▼
┌───────────────────────┐        ┌──────────────────────────┐
│      im-core (Rust)     │        │   extension-runtime (Rust) │
│  Identity/Relationship  │◄──────►│  Manifest/Event Bus订阅/   │
│  Conversation/Message   │ events │  Command分发/隔离沙箱骨架   │
│  （事务边界，PostgreSQL主存）│        │ (MVP: 骨架+审计，无具体扩展)│
└───────────┬─────────────┘        └───────────┬────────────────┘
            │                                   │
            ├───────────────┐                   │
            ▼               ▼                   ▼
   ┌────────────────┐ ┌───────────────┐  ┌─────────────┐
   │  im-presence    │ │   im-media     │  │ NATS JetStream│（Event Bus，全组件共用）
   │ (Rust, Valkey主)│ │ (Rust, MinIO)  │  └─────────────┘
   └────────────────┘ └───────────────┘
            │               │
            ▼               ▼
        Valkey          PostgreSQL / MinIO
```

**拆分依据回填**（呼应 SRS "拆分必须来自 Scaling/Failure/Security/Ownership Boundary"）：
- `im-gateway` 独立：Scaling Boundary（连接数与业务逻辑吞吐特征不同，网关需要横向扩展以承接连接数，im-core 按事务吞吐扩展）。
- `im-presence` 独立：Scaling Boundary（高频写、可容忍最终一致，适合 Valkey 为主存，与 im-core 的强一致 PostgreSQL 事务边界不同）+ Failure Boundary（Presence抖动不应拖慢消息事务）。
- `im-media` 独立：Security Boundary（媒体上传走预签名URL，不应让上传流量路径经过消息主路径）。
- `extension-runtime` 独立：Failure Boundary（SRS `EXT-FR-002` 红线，扩展故障不得影响 Core，进程级隔离的前提是独立部署单元）。
- MVP 阶段 `im-core` 暂不进一步拆分 identity/relationship/conversation/message 子服务——当前负载下没有实测数据支持拆分，遵循"初期禁止过度微服务化"（ADR-011，见第13章）。

## 3. im-core 内部模块划分（单进程内的模块边界，为将来拆分埋点）

```
im-core/
├── identity/       // User, Guest, DeviceSession, Token, AccountState
├── relationship/    // Friend, FriendRequest, Block, Follow, RecentContact
├── conversation/    // Conversation, Membership
├── message/         // Message, Reaction, DeliveryState, Sequence Generator
├── event/           // 领域事件定义 + 发布到 NATS
└── db/              // sqlx models, migrations
```

模块间禁止跨模块直接访问对方的数据库表（每个模块通过自己的 Repository 接口访问自己拥有的表），为未来按此边界拆分为独立服务做准备（呼应 SRS Ownership Boundary）。

## 4. 数据模型（PostgreSQL，核心表，MVP范围）

```sql
-- identity
CREATE TABLE tenants (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE games (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name TEXT NOT NULL,
    UNIQUE (tenant_id, name)
);

CREATE TABLE environments (
    id UUID PRIMARY KEY,
    game_id UUID NOT NULL REFERENCES games(id),
    name TEXT NOT NULL,     -- production / test
    UNIQUE (game_id, name)
);

CREATE TABLE users (
    id UUID PRIMARY KEY,
    environment_id UUID NOT NULL REFERENCES environments(id),
    kind TEXT NOT NULL CHECK (kind IN ('user','guest')),
    external_identity JSONB,        -- {provider, external_uid} 见第6章身份映射
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active','banned','deleted','suspended')),
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (environment_id, external_identity)
);

CREATE TABLE device_sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    device_fingerprint TEXT,
    refresh_token_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

-- relationship（MVP：好友基础模型，可按 tenant/game 配置为禁用，见 SRS IM-REL-002）
CREATE TABLE friendships (
    user_id UUID NOT NULL REFERENCES users(id),
    friend_id UUID NOT NULL REFERENCES users(id),
    state TEXT NOT NULL CHECK (state IN ('pending','accepted','blocked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, friend_id)
);

-- conversation（Core Schema，禁止游戏专有字段，仅 metadata 扩展点）
CREATE TABLE conversations (
    id UUID PRIMARY KEY,
    environment_id UUID NOT NULL REFERENCES environments(id),
    kind TEXT NOT NULL CHECK (kind IN ('dm','group','channel','system','broadcast')),
    metadata JSONB NOT NULL DEFAULT '{}',   -- namespaced: game.*, ai.*, work.*
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE conversation_members (
    conversation_id UUID NOT NULL REFERENCES conversations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    role TEXT NOT NULL DEFAULT 'member',
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_read_sequence BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (conversation_id, user_id)
);

-- message
CREATE TABLE messages (
    id UUID PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES conversations(id),
    sequence BIGINT NOT NULL,          -- 会话内单调递增，见第5章
    sender_id UUID REFERENCES users(id),   -- NULL = 系统消息
    kind TEXT NOT NULL CHECK (kind IN ('text','image','file','sticker','system','custom')),
    content JSONB NOT NULL,
    reply_to UUID REFERENCES messages(id),
    idempotency_key TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'sent' CHECK (state IN ('sent','delivered','read','recalled','deleted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at TIMESTAMPTZ,
    UNIQUE (conversation_id, sequence),
    UNIQUE (conversation_id, sender_id, idempotency_key)
);

CREATE TABLE message_reactions (
    message_id UUID NOT NULL REFERENCES messages(id),
    user_id UUID NOT NULL REFERENCES users(id),
    emoji TEXT NOT NULL,
    PRIMARY KEY (message_id, user_id, emoji)
);

-- audit（呼应 SEC-NFR-006）
CREATE TABLE audit_logs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    actor_id UUID,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id UUID,
    detail JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

索引策略：`messages(conversation_id, sequence)` 为核心查询路径（增量同步），`conversation_members(user_id)` 支持"我的会话列表"查询。分区策略（按 `conversation_id` hash 或按时间）留待容量评估后在详细设计中确定（**ADR REQUIRED**，非MVP阻塞项，MVP先用普通表）。

## 5. 消息 Sequence 与幂等设计

- **Sequence 生成**：每个 Conversation 的 Sequence 由 PostgreSQL 内 `SELECT ... FOR UPDATE` 行锁 + 自增列，或使用单独的 `conversation_sequences(conversation_id, next_seq)` 表在同一事务内原子递增后写入 `messages`，保证同一 Conversation 内严格单调、无重复（呼应 `IM-FR-002`）。**MVP 采用行锁方案**（实现简单，正确性优先于极限吞吐），若后续压测显示为瓶颈，再评估分布式ID方案（ADR候选，非MVP决策点）。
- **幂等**：客户端生成 `idempotency_key`（如 UUID），与 `(conversation_id, sender_id)` 联合唯一约束；重复提交返回已存在的消息而非报错，语义为"幂等成功"。
- **Delivery State 状态机**：`sent → delivered → read`，允许旁路到 `recalled`/`deleted`；状态转换记录经由 NATS 事件驱动 `im-presence`/推送逻辑异步更新，不阻塞发送主路径（呼应 IM Core 短路径原则）。

## 6. Identity / Token 设计

```
POST /v1/auth/token/exchange   (Server-to-Server, 游戏服务器调用，携带 Server Secret)
  body: { environment_id, external_provider, external_uid, display_name? }
  → { access_token, refresh_token, user_id }
```

- **Server Secret**：按 Environment 独立签发，存储于 im-core 侧做 HMAC 校验；游戏客户端**永不持有** Server Secret（呼应 `GAME-ID-003` 红线）。
- **Access Token**：JWT，短生命周期（Candidate: 15分钟，**BENCHMARK REQUIRED**），claims 包含 `user_id, environment_id, tenant_id, exp`。
- **Refresh Token**：与 `device_sessions.refresh_token_hash` 绑定，支持 Rotation（每次刷新旧 Token 失效）。
- **Guest**：客户端直接调用 `POST /v1/auth/guest`（无需 Server Secret，但受 im-gateway Rate Limit 保护），生成 `kind='guest'` 的 User。
- **Guest Upgrade**：`POST /v1/auth/link`，携带 Guest Access Token + 外部身份凭证，若目标外部身份已存在 User 则触发 `ISS-VOICE`同类型的 **Account Merge 冲突流程**（MVP：拒绝合并并返回冲突错误，交由客户端提示用户选择；自动合并策略非MVP范围，标记 Open Issue）。

## 7. Conversation / Message API 草案（REST，MVP）

```
POST   /v1/conversations                       创建会话（kind=dm|group|channel）
GET    /v1/conversations                        我的会话列表（分页，含last_message摘要）
GET    /v1/conversations/{id}/messages?after_sequence=&limit=   增量拉取（离线同步核心接口）
POST   /v1/conversations/{id}/messages          发送消息（需idempotency_key）
PATCH  /v1/conversations/{id}/messages/{msg_id} 编辑消息
POST   /v1/conversations/{id}/messages/{msg_id}/recall   撤回（时间窗校验）
POST   /v1/conversations/{id}/messages/{msg_id}/reactions  添加reaction
POST   /v1/conversations/{id}/read              上报已读（更新last_read_sequence）
```

实时通道（WebSocket，im-gateway 终结）：

```
Client → Gateway:  { type: "send_message", conversation_id, idempotency_key, content }
Gateway → Client:  { type: "message_ack", idempotency_key, message_id, sequence }
Gateway → Client:  { type: "message_new", message }          // 广播给会话在线成员
Gateway → Client:  { type: "presence_update", user_id, status }
Gateway → Client:  { type: "typing", conversation_id, user_id }
```

**离线/断线重连**：客户端携带 `last_known_sequence`（每会话）通过 `GET /v1/conversations/{id}/messages?after_sequence=` 增量拉取，WebSocket 仅推送重连后的新增；不做"服务端主动补发离线队列"这类有状态设计，退化为客户端拉取模式，实现更简单且天然幂等（呼应弱网设计原则，MVP优先简单正确）。

## 8. 事件总线（NATS JetStream）Topic 设计

```
im.message.created         { message_id, conversation_id, sender_id, sequence }
im.message.recalled        { message_id, conversation_id }
im.conversation.created    { conversation_id, kind, metadata }
im.conversation.member_joined / member_left
im.presence.changed        { user_id, status }
im.identity.state_changed  { user_id, state }   // ban/delete等，供audit与其他服务响应
```

Extension Runtime（骨架）通过 JetStream Consumer 订阅上述 Topic 的子集（按 Manifest 声明的 `event_subscription` 过滤），**MVP 阶段无任何 Extension 实际消费这些事件**，仅验证订阅链路可用性与权限隔离（呼应 `EXT-FR-001`）。

## 9. im-gateway 设计要点

- 单个 WebSocket 连接对应一个 `device_session`；同一 User 允许多端同时连接（多设备），但 Token 校验/续期独立。
- 连接状态（`user_id → gateway_instance_id → connection_id`）写入 Valkey，供 Fan-out 时定位消息应推送到哪个 Gateway 实例（多网关实例横向扩展的基础）。
- Fan-out 路径：`im-core` 写入消息并提交事务后，发布 `im.message.created` 事件 → 订阅该事件的 Fan-out 组件（MVP 阶段可内置于 im-gateway 或作为 im-core 的轻量 sidecar 逻辑，**不新建独立服务**，避免过早拆分）→ 查询 Valkey 中会话在线成员的 Gateway 位置 → 通过内部 gRPC 推送到对应 Gateway 实例 → WebSocket 下发。
- Rate Limit：按 `user_id` + `ip` 维度，基于 Valkey 令牌桶实现，MVP 覆盖发送消息、创建会话、Guest 注册三个高风险端点。

## 10. Media（im-media）设计要点

```
POST /v1/media/presign   { content_type, size_hint } → { upload_url, media_id, expires_at }
```

- 客户端直接 PUT 到 MinIO 预签名 URL，不经过 im-core/im-gateway（呼应 `IM-MEDIA-002`）。
- 消息 `content` 中仅存 `media_id` 引用，展示时客户端调用 `GET /v1/media/{id}` 换取短期可读 URL。
- MVP 不做病毒扫描/内容审核（Moderation Extension 范围，V1+）。

## 11. Game SDK 五步接入映射到本设计的实际调用

```
Voice/Game SDK.Initialize(config)      → 加载 environment_id / gateway endpoint 配置
SDK.Authenticate(server_issued_token)  → 客户端使用游戏服务器下发的 access_token（服务器已完成 §6 Token Exchange）
SDK.Connect()                          → 建立 WebSocket 到 im-gateway，携带 access_token
SDK.Join(conversation_id)              → 订阅会话（服务端校验 conversation_members 或按metadata自动加入逻辑）
SDK.Send(conversation_id, content)     → 上文 WebSocket send_message
```

Unity 官方 SDK（MVP交付形态）：C# 封装上述五步 + 本地消息缓存 + 自动重连（指数退避）+ 心跳（30s间隔，**BENCHMARK REQUIRED**确认合适间隔）。

## 12. Laser HUD（MVP范围：Desktop独立窗口方案）

依据 SRS `HUD-OV-002`（默认禁止Hook）与 MVP 优先级，**MVP 阶段 HUD 采用独立置顶窗口方案**（非游戏内渲染插件），跨引擎零集成成本，规避反作弊风险，代价是无法在游戏独占全屏(exclusive fullscreen，非borderless)模式下正确置顶——此限制需在MVP发布说明中明确告知，Engine Plugin方案作为V1的追加能力（覆盖独占全屏场景），而非推翻MVP方案重做。

```
HUD Runtime (Tauri 或等效轻量webview容器, 待技术选型ADR)
   │
   ├─ Rust Core（复用 SDK 的连接/状态管理逻辑）
   └─ Next.js构建产物作为WebView内容（复用Dashboard的UI组件库，降低维护成本）
```

Laser层四态（Idle/NewMessage/Mention/Priority）→ Peek/Chat 层复用 Next.js 组件，Full Client 暂不在 MVP 交付（用 Web Dashboard 替代 Full Client 的管理类功能）。

## 13. K3s 部署清单（MVP）

```
Namespace: im-platform
Deployments:
  im-gateway         (replicas: 2+, HPA on connection count)
  im-core            (replicas: 2+, HPA on CPU, 无状态，PG为状态)
  im-presence        (replicas: 2+)
  im-media           (replicas: 1-2)
  extension-runtime  (replicas: 1, MVP骨架，低负载)
  web-dashboard      (Next.js, replicas: 2+)
StatefulSets/Operators:
  postgres (建议CloudNativePG Operator，Apache-2.0/PostgreSQL License，符合SRS开源约束)
  nats (JetStream, NATS官方Helm Chart)
  valkey (Valkey官方或社区Helm Chart)
  minio (MinIO Operator)
Ingress:
  gateway.{tenant}.example.com → im-gateway (WSS)
  api.{tenant}.example.com     → im-gateway HTTP路由 / im-core内部API不直接暴露
  dashboard.{tenant}.example.com → web-dashboard
```

**ADR-012（新增）**：PostgreSQL Operator 选型（CloudNativePG vs Zalando Postgres Operator vs 手工StatefulSet）—— Candidate: CloudNativePG（活跃维护、Apache-2.0），Needs PoC 验证故障切换行为（呼应 SRS VOICE-POC-009 同类验证方法论）。

## 14. 配置与密钥管理

- 每 Environment 独立 `server_secret`（第6章）、JWT签名密钥，存储于 K3s Secret（MVP），V1+ 评估外部KMS（如开源Vault，Apache-2.0/MPL，符合开源约束）。
- 租户级配置（Rate Limit阈值、好友系统开关、消息保留策略）存于 `environments` 表的 `settings JSONB` 列（本设计文档第4章表结构可扩展此列，实现时补充），由 im-core 加载并缓存至 Valkey。

## 15. 可观测性落地（MVP最小集）

- OpenTelemetry Collector 部署为 K3s DaemonSet 或 Deployment，接收 im-gateway/im-core/im-presence 的 Trace/Metrics。
- MVP 落地指标（SRS第37章全集的子集）：`active_connections, message_latency, message_delivery_latency, connection_latency, reconnect_rate`。其余指标（`extension_latency`等）待Extension有实际负载后再接入。

## 16. 详细设计阶段的遗留决策点（非本文档阻塞项，需在下一阶段闭环）

- ADR-011：im-core 未来拆分的触发阈值（QPS/连接数具体数字，需第一轮压测后回填）。
- ADR-012：PostgreSQL Operator 选型（第13章）。
- ADR-013：HUD Runtime 容器技术选型（Tauri vs 其他，第12章）。
- Sequence生成方案的性能天花板需要 POC-02（SRS第47章）实测后确认是否需要升级为分布式方案。
- `settings JSONB` 的具体字段清单（好友开关、保留策略等）需在详细设计阶段与产品逐项确认字段命名与默认值。

---

## 附录：与 SRS/LiveKit 文档的一致性检查

- 本设计未在 `conversations`/`messages` 表中引入任何游戏专有字段，符合 SRS `IM-CONV-002`。
- Extension Runtime 骨架已预留事件订阅隔离机制，符合 `EXT-FR-002`，但具体沙箱强度（进程级/WASM）留待 ADR-005 决议后在详细设计中落地，本设计不预先假设。
- Voice Control Service（LiveKit文档）与本设计的 im-core 共享租户/用户模型，但作为独立服务不在本版本部署清单中（V1加入），本设计的 `users`/`environments` 表结构已预留被语音子系统复用的兼容性（无需修改即可被引用）。
