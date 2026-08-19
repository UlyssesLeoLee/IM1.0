# 可嵌入式游戏 IM 平台 详细设计书（Detailed Design）

版本：v0.1 Draft ｜ 范围：**MVP**（承接 `docs/BasicDesign.md`）
本文档面向编码落地：完整 REST/WebSocket 协议、内部 gRPC 契约、错误码表、数据库迁移脚本骨架、模块级 Rust 接口签名、状态机定义、配置项清单。不重复 BasicDesign 已给出的架构图与拆分依据，只做细化。

---

## 1. 代码仓库结构（Cargo Workspace + Next.js）

```
/
├── Cargo.toml                 # workspace root
├── crates/
│   ├── im-core/
│   ├── im-gateway/
│   ├── im-presence/
│   ├── im-media/
│   ├── extension-runtime/
│   ├── im-proto/              # 共享 protobuf 定义 + 生成代码（tonic-build）
│   └── im-common/             # 共享工具：错误类型、tracing初始化、配置加载
├── migrations/                 # sqlx migrate，单一目录，按模块前缀命名文件
├── web/
│   └── dashboard/              # Next.js App Router 项目
├── deploy/
│   └── k3s/                    # Helm charts / raw manifests
└── docs/
```

- `im-proto` 是 `im-gateway ⇄ im-core ⇄ extension-runtime` 内部通信的唯一契约来源，禁止任何服务手写与其重复的结构体（避免契约漂移）。
- Web Dashboard 独立 Next.js 项目，通过 REST 调用 `im-gateway` 暴露的 HTTP API（不直接连数据库）。

## 2. 内部 gRPC 契约（im-proto，节选，Protobuf）

```protobuf
// im-proto/core.proto
syntax = "proto3";
package im.core.v1;

service CoreService {
  rpc ExchangeToken(ExchangeTokenRequest) returns (ExchangeTokenResponse);
  rpc AuthenticateGuest(AuthenticateGuestRequest) returns (AuthenticateGuestResponse);
  rpc RefreshToken(RefreshTokenRequest) returns (RefreshTokenResponse);
  rpc ValidateAccessToken(ValidateAccessTokenRequest) returns (ValidateAccessTokenResponse);

  rpc CreateConversation(CreateConversationRequest) returns (Conversation);
  rpc ListConversations(ListConversationsRequest) returns (ListConversationsResponse);
  rpc SendMessage(SendMessageRequest) returns (Message);
  rpc ListMessages(ListMessagesRequest) returns (ListMessagesResponse);
  rpc EditMessage(EditMessageRequest) returns (Message);
  rpc RecallMessage(RecallMessageRequest) returns (google.protobuf.Empty);
  rpc ReactMessage(ReactMessageRequest) returns (google.protobuf.Empty);
  rpc MarkRead(MarkReadRequest) returns (google.protobuf.Empty);

  rpc SendFriendRequest(SendFriendRequestRequest) returns (google.protobuf.Empty);
  rpc RespondFriendRequest(RespondFriendRequestRequest) returns (google.protobuf.Empty);
  rpc BlockUser(BlockUserRequest) returns (google.protobuf.Empty);
}

message ValidateAccessTokenRequest { string access_token = 1; }
message ValidateAccessTokenResponse {
  bool valid = 1;
  string user_id = 2;
  string environment_id = 3;
  string tenant_id = 4;
  int64 expires_at_unix = 5;
}

message SendMessageRequest {
  string conversation_id = 1;
  string sender_id = 2;
  string idempotency_key = 3;
  MessageKind kind = 4;
  bytes content_json = 5;      // 见第4章 content schema
  optional string reply_to = 6;
}

enum MessageKind { TEXT = 0; IMAGE = 1; FILE = 2; STICKER = 3; SYSTEM = 4; CUSTOM = 5; }
```

`im-gateway` 是唯一的 gRPC 客户端调用方（对客户端而言的服务端），`im-core` 是唯一的服务端实现方；`extension-runtime` 仅作为 NATS 事件订阅方，不直接调用 `CoreService`（避免扩展绕过审计/权限层直接操纵核心数据，呼应 SRS `EXT-FR-002`）。

## 3. WebSocket 协议完整定义（im-gateway ⇄ 客户端）

帧格式：JSON over WebSocket Text Frame（MVP选择JSON而非二进制Protobuf，优先开发效率与可调试性；若压测显示序列化开销显著，可在detailed design修订版切换为二进制，标记为**ADR-014 Candidate**）。

### 3.1 客户端 → 服务端

```json
{ "type": "auth", "access_token": "..." }
{ "type": "send_message", "req_id": "uuid", "conversation_id": "...", "idempotency_key": "...", "kind": "text", "content": {"text": "hi"}, "reply_to": null }
{ "type": "edit_message", "req_id": "uuid", "message_id": "...", "content": {"text": "edited"} }
{ "type": "recall_message", "req_id": "uuid", "message_id": "..." }
{ "type": "react", "req_id": "uuid", "message_id": "...", "emoji": "👍" }
{ "type": "mark_read", "conversation_id": "...", "sequence": 123 }
{ "type": "typing", "conversation_id": "..." }
{ "type": "ping", "ts": 1234567890 }
```

### 3.2 服务端 → 客户端

```json
{ "type": "connected", "session_id": "..." }
{ "type": "ack", "req_id": "uuid", "ok": true, "data": { "message_id": "...", "sequence": 42 } }
{ "type": "ack", "req_id": "uuid", "ok": false, "error": { "code": "IDEMPOTENCY_CONFLICT", "message": "..." } }
{ "type": "message_new", "message": { "...": "见第4章Message JSON Schema" } }
{ "type": "message_edited", "message_id": "...", "content": {...}, "edited_at": "..." }
{ "type": "message_recalled", "message_id": "..." }
{ "type": "reaction_added", "message_id": "...", "user_id": "...", "emoji": "👍" }
{ "type": "presence_update", "user_id": "...", "status": "online" }
{ "type": "typing", "conversation_id": "...", "user_id": "..." }
{ "type": "pong", "ts": 1234567890 }
{ "type": "force_disconnect", "reason": "token_revoked" }
```

**协议规则**：
- 所有客户端发起的写操作携带 `req_id`（客户端生成 UUID），服务端 `ack` 必须回带同一 `req_id`，客户端据此匹配请求-响应，支持并发多请求在途。
- `ack.ok=false` 时 `error.code` 取值见第7章错误码表，客户端据 code 做分支处理（如 `IDEMPOTENCY_CONFLICT` 直接视为成功，因为同 key 已被处理）。
- 心跳：客户端每 30s 发送 `ping`，服务端 60s 未收到任何帧则视为死连接主动断开（呼应 BasicDesign 第11章 SDK 心跳设计）。

## 4. 数据 Schema（JSON，MVP范围）

```json
// Message.content（kind=text）
{ "text": "string, max 4000 chars" }

// Message.content（kind=image）
{ "media_id": "uuid", "width": 0, "height": 0, "thumbnail_media_id": "uuid?" }

// Message JSON（下行完整结构）
{
  "id": "uuid",
  "conversation_id": "uuid",
  "sequence": 42,
  "sender_id": "uuid|null",
  "kind": "text",
  "content": { "...": "..." },
  "reply_to": "uuid|null",
  "state": "sent",
  "created_at": "2026-08-19T00:00:00Z",
  "edited_at": null,
  "reactions": [{ "emoji": "👍", "user_ids": ["uuid"] }]
}

// Conversation.metadata 命名空间约定（呼应 SRS IM-CONV-002）
{
  "game.type": "guild|party|match|...",   // 由 Game Extension 写入，Core 不解释语义
  "game.game_id": "uuid",
  "game.external_group_id": "string"
}
```

## 5. REST API 完整清单（im-gateway 对外暴露，MVP）

| Method | Path | 说明 | 认证 |
|---|---|---|---|
| POST | `/v1/auth/token/exchange` | Server-to-Server 身份换取 | Server Secret (HMAC签名) |
| POST | `/v1/auth/guest` | Guest 注册 | 无（Rate Limit保护） |
| POST | `/v1/auth/refresh` | 刷新 Access Token | Refresh Token |
| POST | `/v1/auth/link` | Guest Upgrade | Access Token + 外部凭证 |
| POST | `/v1/auth/logout` | 撤销当前 Device Session | Access Token |
| GET | `/v1/conversations` | 我的会话列表（分页） | Access Token |
| POST | `/v1/conversations` | 创建会话 | Access Token |
| GET | `/v1/conversations/{id}/messages` | 增量/历史拉取（`after_sequence`, `limit`） | Access Token + 成员校验 |
| GET | `/v1/conversations/{id}/members` | 会话成员列表 | Access Token + 成员校验 |
| POST | `/v1/friends/requests` | 发送好友申请 | Access Token |
| POST | `/v1/friends/requests/{id}/respond` | 接受/拒绝 | Access Token |
| POST | `/v1/friends/{id}/block` | 拉黑 | Access Token |
| GET | `/v1/friends` | 好友列表 | Access Token |
| POST | `/v1/media/presign` | 获取媒体上传预签名URL | Access Token |
| GET | `/v1/media/{id}` | 获取媒体可读URL | Access Token + 权限校验 |
| GET | `/v1/me` | 当前用户资料 | Access Token |
| PATCH | `/v1/me` | 更新资料（display_name等） | Access Token |
| GET | `/healthz` | 健康检查 | 无 |
| GET | `/metrics` | Prometheus指标 | 内网限制 |

分页统一约定：`?cursor=<opaque>&limit=<n, default 50, max 200>`，响应体含 `next_cursor: string|null`。

## 6. 状态机定义

### 6.1 Message.state

```
sent ──(投递成功)──▶ delivered ──(读取)──▶ read
 │                                            
 ├──(作者操作,时间窗内)──▶ recalled          
 └──(作者/管理员操作)────▶ deleted
```
非法转换（如从 `recalled` 转回 `sent`）在 `im-core` 内以 Rust `enum` + `match` 穷尽校验，非法转换返回 `INVALID_STATE_TRANSITION`。

### 6.2 User.state

```
active ──(封禁)──▶ banned ──(解封)──▶ active
active ──(账号删除请求)──▶ deleted（终态，不可逆）
active ──(风控)──▶ suspended ──(复核通过)──▶ active
```
`banned`/`suspended`/`deleted` 状态下拒绝该用户的所有写操作（发消息/加好友等），由 `im-core` 在每次写请求入口统一校验（避免各处散落检查逻辑遗漏，集中在单一 middleware/interceptor 内实现）。

### 6.3 DeviceSession 生命周期

```
created ──(refresh)──▶ rotated(新session_id, 旧refresh_token_hash失效)
created/rotated ──(logout | 检测到新登录挤占 | 管理员强制下线)──▶ revoked
```

## 7. 错误码表（MVP，节选，贯穿REST与WS的ack.error.code）

| Code | HTTP状态 | 说明 | 客户端建议处理 |
|---|---|---|---|
| `UNAUTHORIZED` | 401 | Token缺失/无效/过期 | 触发refresh或重新登录 |
| `FORBIDDEN` | 403 | 无权限访问该资源（非会话成员等） | 提示无权限，不重试 |
| `NOT_FOUND` | 404 | 资源不存在 | 提示不存在 |
| `IDEMPOTENCY_CONFLICT` | 200（WS: ok=true特殊语义） | 幂等键已处理，返回已有结果 | 视为成功 |
| `RATE_LIMITED` | 429 | 触发限流 | 退避重试，读取`Retry-After` |
| `INVALID_STATE_TRANSITION` | 409 | 状态机非法转换（如已撤回消息再次撤回） | 刷新本地状态 |
| `RECALL_WINDOW_EXPIRED` | 409 | 撤回时间窗已过 | 提示改用删除 |
| `ACCOUNT_BANNED` | 403 | 账号被封禁 | 展示封禁信息 |
| `ACCOUNT_MERGE_CONFLICT` | 409 | Guest Upgrade时目标身份已存在 | 提示用户选择处理方式（MVP不自动合并） |
| `VALIDATION_ERROR` | 400 | 请求体校验失败 | 展示具体字段错误 |
| `INTERNAL_ERROR` | 500 | 服务端内部错误 | 重试（幂等操作）或提示稍后再试 |
| `SERVICE_UNAVAILABLE` | 503 | 依赖服务（PG/NATS等）暂时不可用 | 指数退避重试 |

## 8. 数据库迁移脚本骨架（sqlx migrate 命名约定）

```
migrations/
  0001_create_tenants_games_environments.sql
  0002_create_users_device_sessions.sql
  0003_create_friendships.sql
  0004_create_conversations_members.sql
  0005_create_messages_reactions.sql
  0006_create_audit_logs.sql
  0007_add_environments_settings_jsonb.sql   -- BasicDesign §14 提及的租户级配置列
```
每个迁移文件为 `sqlx` 标准的 `-- +migrate Up` / `-- +migrate Down` 双向脚本（或采用 sqlx 默认的仅 `up.sql`/`down.sql` 双文件约定，具体由实现时选用的 sqlx-cli 版本约定决定，非架构决策，留给编码阶段）。表结构定义见 `docs/BasicDesign.md` 第4章，此处不重复，迁移文件应逐字对应。

## 9. Rust 模块接口签名（核心示例，非全量，用于统一编码风格）

```rust
// crates/im-core/src/message/repository.rs
#[async_trait]
pub trait MessageRepository {
    async fn insert(&self, tx: &mut PgTransaction, msg: NewMessage) -> Result<Message, MessageError>;
    async fn find_by_idempotency_key(
        &self, conversation_id: ConversationId, sender_id: UserId, key: &str,
    ) -> Result<Option<Message>, MessageError>;
    async fn list_after_sequence(
        &self, conversation_id: ConversationId, after: i64, limit: i32,
    ) -> Result<Vec<Message>, MessageError>;
    async fn update_state(
        &self, tx: &mut PgTransaction, message_id: MessageId, new_state: MessageState,
    ) -> Result<(), MessageError>;
}

// crates/im-core/src/message/service.rs
pub struct MessageService<R: MessageRepository, S: SequenceAllocator, E: EventPublisher> {
    repo: R,
    sequencer: S,
    events: E,
}

impl<R, S, E> MessageService<R, S, E>
where R: MessageRepository, S: SequenceAllocator, E: EventPublisher {
    pub async fn send_message(&self, cmd: SendMessageCommand) -> Result<Message, MessageError> {
        // 1. 幂等检查（find_by_idempotency_key）
        // 2. 会话成员权限校验（由调用方/中间层完成，此处假设已校验）
        // 3. 开事务：sequencer.next(conversation_id) + repo.insert
        // 4. 提交事务后 events.publish(MessageCreated{..})
        // 5. 返回 Message
        todo!()
    }
}
```

`MessageError` 枚举与第7章错误码表一一映射，在 `im-gateway` 边界统一转换为 HTTP状态码/WS error.code（保证错误语义单一来源，不在多处重复定义映射关系）。

## 10. 配置项清单（环境变量，MVP）

| 变量 | 说明 | 默认值/示例 |
|---|---|---|
| `IM_DATABASE_URL` | PostgreSQL 连接串 | — |
| `IM_VALKEY_URL` | Valkey 连接串 | — |
| `IM_NATS_URL` | NATS 连接串 | — |
| `IM_MINIO_ENDPOINT` / `IM_MINIO_ACCESS_KEY` / `IM_MINIO_SECRET_KEY` | MinIO 凭证 | — |
| `IM_JWT_SIGNING_KEY` | Access Token 签名密钥（按Environment细分，见BasicDesign§14） | — |
| `IM_ACCESS_TOKEN_TTL_SECONDS` | Access Token 有效期 | `900`（Candidate，BENCHMARK REQUIRED） |
| `IM_WS_HEARTBEAT_TIMEOUT_SECONDS` | WS心跳超时 | `60` |
| `IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN` | 单用户每分钟发送消息上限 | `60`（Candidate） |
| `IM_MESSAGE_RECALL_WINDOW_SECONDS` | 撤回时间窗 | `120`（Candidate） |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry Collector地址 | — |

所有 `Candidate` 标注的默认值需在 SRS PoC-02 / 压测阶段验证后固化，本文档不视为最终值。

## 11. 与上游文档的追溯关系

| 本文档章节 | 对应 SRS 需求ID | 对应 BasicDesign章节 |
|---|---|---|
| §3 WS协议 | IM-FR-002/003/004 | §7 |
| §6.1 Message状态机 | IM-FR-004 | §5 |
| §6.2 User状态机 | IM-ID-004 | §4 |
| §7 错误码 IDEMPOTENCY_CONFLICT | IM-FR-003 | §5 |
| §5 REST /v1/auth/token/exchange | GAME-ID-003 | §6 |
| §4 Conversation.metadata命名空间 | IM-CONV-002 | §4 |

## 12. 遗留待确认项（编码阶段前需闭环，非文档完整性阻塞项）

- WS帧序列化格式（JSON vs Protobuf二进制）：ADR-014，Candidate暂定JSON。
- sqlx迁移双向脚本 vs 单向脚本约定：交由实现时选定的工具链版本决定。
- Rate Limit / Token TTL / 撤回时间窗的具体数值：均为Candidate，压测与产品确认后回填。
