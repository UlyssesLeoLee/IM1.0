# 可嵌入式游戏 IM 平台 详细设计书（Detailed Design）

版本:v0.2(2026-08-27 升版,承接 v0.1 Draft) | 范围:**MVP**(承接 `docs/BasicDesign.md`)
本文档面向编码落地:完整 REST/WebSocket 协议、内部 gRPC 契约、错误码表、数据库迁移脚本骨架、模块级 Rust 接口签名、状态机定义、配置项清单。不重复 BasicDesign 已给出的架构图与拆分依据,只做细化。

**v0.2 变更**:新增 §X「Bevy 客户端 ECS 模块设计」(草案,待 DDD Review)。v0.1 Draft §1-§12 内容**不**改动。WS / gRPC / REST 协议细节沿用 v0.1 Draft。

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
| GET | `/healthz` | 健康检查(进程存活) | 无 |
| GET | `/readyz` | 就绪检查(PG/Valkey/NATS 全部可达才 200) | 无 |
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
| `FRIEND_REQUEST_EXISTS` | 409 | 已存在待处理的好友申请 | 提示已发送申请 |
| `USER_BLOCKED` | 403 | 目标用户已在黑名单或被对方屏蔽 | 提示无法发起操作 |
| `VALIDATION_ERROR` | 400 | 请求体校验失败 | 展示具体字段错误 |
| `INTERNAL_ERROR` | 500 | 服务端内部错误 | 重试（幂等操作）或提示稍后再试 |
| `SERVICE_UNAVAILABLE` | 503 | 依赖服务（PG/NATS等）暂时不可用 | 指数退避重试 |

## 8. 数据库迁移脚本骨架（sqlx migrate 命名约定）

```
migrations/
  0001_create_tenants_games_environments.sql
  0002_create_users_device_sessions.sql
  0003_create_friend_requests_and_friendships.sql
  0004_create_conversations_sequences_members_dm_pairs.sql
  0005_create_messages_reactions.sql
  0006_create_audit_logs.sql
```
每个迁移文件为 `sqlx` 标准的 `-- +migrate Up` / `-- +migrate Down` 双向脚本（或采用 sqlx 默认的仅 `up.sql`/`down.sql` 双文件约定，具体由实现时选用的 sqlx-cli 版本约定决定，非架构决策，留给编码阶段）。表结构定义见 `docs/BasicDesign.md` 第4章，此处不重复，迁移文件应逐字对应。

## 9. Rust 模块接口签名(核心示例,非全量,用于统一编码风格)

> 完整 trait / struct 列表见 `ImplementationSpec.md §3`(从详细设计衍生的实施规范,编码可直接对照实现)。

### 9.1 Message 模块(主路径,带完整实现)

```rust
// crates/im-core/src/message/repository.rs
#[async_trait]
pub trait MessageRepository: Send + Sync {
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

// crates/im-core/src/message/sequence.rs
#[async_trait]
pub trait SequenceAllocator: Send + Sync {
    /// 行锁原子自增;若 conversation_sequences 行不存在则先 INSERT。
    async fn next(&self, tx: &mut PgTransaction, conversation_id: ConversationId) -> Result<i64, MessageError>;
}

// crates/im-core/src/event/publisher.rs
#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), EventError>;
}

// crates/im-core/src/message/service.rs
pub struct MessageService<R, S, E>
where
    R: MessageRepository,
    S: SequenceAllocator,
    E: EventPublisher,
{
    repo: R,
    sequencer: S,
    events: E,
}

#[derive(Debug)]
pub struct SendMessageCommand {
    pub conversation_id: ConversationId,
    pub sender_id: UserId,
    pub idempotency_key: String,
    pub kind: MessageKind,
    pub content: serde_json::Value,
    pub reply_to: Option<MessageId>,
}

impl<R, S, E> MessageService<R, S, E>
where
    R: MessageRepository,
    S: SequenceAllocator,
    E: EventPublisher,
{
    pub async fn send_message(&self, cmd: SendMessageCommand) -> Result<Message, MessageError> {
        // 1. 幂等检查:同一 (conversation, sender, idempotency_key) 已存在则直接返回
        if let Some(existing) = self.repo
            .find_by_idempotency_key(cmd.conversation_id, cmd.sender_id, &cmd.idempotency_key)
            .await?
        {
            return Ok(existing);  // 视为幂等成功,对应 aux-03 IDEMPOTENCY_CONFLICT 语义
        }

        // 2. 校验 content schema(由 validator 注入)
        validate_content(&cmd.kind, &cmd.content)?;

        // 3. 开事务:取 sequence + insert
        let mut tx = self.repo.begin_tx().await?;
        let sequence = self.sequencer.next(&mut tx, cmd.conversation_id).await?;
        let new_msg = NewMessage {
            id: MessageId::new(),
            conversation_id: cmd.conversation_id,
            sequence,
            sender_id: Some(cmd.sender_id),
            kind: cmd.kind,
            content: cmd.content,
            reply_to: cmd.reply_to,
            idempotency_key: cmd.idempotency_key,
            state: MessageState::Sent,
        };
        let msg = self.repo.insert(&mut tx, new_msg).await?;
        tx.commit().await?;

        // 4. 提交事务后发布事件(事务外,失败不阻塞 ack 但写入 DLQ)
        let event = MessageCreatedEvent {
            message_id: msg.id,
            conversation_id: msg.conversation_id,
            sender_id: msg.sender_id,
            sequence: msg.sequence,
            kind: msg.kind,
        };
        if let Err(e) = self.events.publish(
            "im.message.created",
            &serde_json::to_vec(&event).unwrap(),
        ).await {
            tracing::error!(error = %e, message_id = %msg.id, "publish failed, will be retried by outbox");
            // V1 引入 outbox 表;MVP 记录日志,SRE 手动重放
        }

        // 5. 返回完整 Message
        Ok(msg)
    }
}
```

### 9.2 Identity 模块(签名级,实现由 ImplementationSpec 落地)

```rust
// crates/im-core/src/identity/repository.rs
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_external_identity(
        &self, env: EnvironmentId, provider: &str, external_uid: &str,
    ) -> Result<Option<User>, IdentityError>;
    async fn create(&self, env: EnvironmentId, kind: UserKind, external: Option<ExternalIdentity>, display_name: Option<String>) -> Result<User, IdentityError>;
    async fn get(&self, id: UserId) -> Result<Option<User>, IdentityError>;
    async fn update_state(&self, id: UserId, state: UserState) -> Result<(), IdentityError>;
}

// crates/im-core/src/identity/token.rs
pub struct TokenService {
    signing_keys: Vec<SigningKey>,  // 至少 1 个;轮换期有 v1+v2
    access_ttl: Duration,
    refresh_pepper: SecretString,
}

impl TokenService {
    pub fn issue_access_token(&self, user: &User) -> Result<AccessToken, IdentityError>;
    pub fn validate_access_token(&self, token: &str) -> Result<TokenClaims, IdentityError>;
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenPair, IdentityError>;
    pub async fn revoke_device_session(&self, session_id: DeviceSessionId) -> Result<(), IdentityError>;
}
```

### 9.3 Conversation 模块(签名级)

```rust
// crates/im-core/src/conversation/service.rs
pub struct ConversationService {
    repo: Arc<dyn ConversationRepository>,
    events: Arc<dyn EventPublisher>,
}

impl ConversationService {
    pub async fn create_dm(&self, env: EnvironmentId, user_a: UserId, user_b: UserId) -> Result<Conversation, ConversationError>;
    pub async fn create_group(&self, env: EnvironmentId, creator: UserId, members: Vec<UserId>, metadata: serde_json::Value) -> Result<Conversation, ConversationError>;
    pub async fn list_user_conversations(&self, user: UserId, cursor: Option<Cursor>, limit: i32) -> Result<Vec<Conversation>, ConversationError>;
    pub async fn add_member(&self, conv: ConversationId, user: UserId) -> Result<(), ConversationError>;
    pub async fn remove_member(&self, conv: ConversationId, user: UserId) -> Result<(), ConversationError>;
    pub async fn is_member(&self, conv: ConversationId, user: UserId) -> Result<bool, ConversationError>;
}
```

### 9.4 im-gateway WebSocket 会话(签名级)

```rust
// crates/im-gateway/src/ws/session.rs
pub struct WsSession {
    user_id: UserId,
    environment_id: EnvironmentId,
    device_session_id: DeviceSessionId,
    outbound: mpsc::UnboundedSender<OutboundFrame>,
}

impl WsSession {
    /// 启动 WS 循环:读帧 → 路由 → 处理 → 写回 outbound
    pub async fn run(
        self,
        conn: actix_ws::MessageStream,
        sink: actix_ws::Session,
        router: Arc<FrameRouter>,
    ) -> Result<(), WsError>;

    /// 应用层心跳;被 timer 触发
    pub fn on_ping(&self, ts: i64) -> Result<(), WsError>;

    /// 服务端 push(由 im-core 事件经 NATS / internal gRPC 触发)
    pub fn push_message_new(&self, msg: Message) -> Result<(), WsError>;
    pub fn push_presence_update(&self, user: UserId, status: PresenceStatus) -> Result<(), WsError>;
    pub fn force_disconnect(&self, reason: ForceDisconnectReason) -> Result<(), WsError>;
}
```

### 9.5 错误统一映射

```rust
// crates/im-common/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("identity: {0}")] Identity(#[from] IdentityError),
    #[error("conversation: {0}")] Conversation(#[from] ConversationError),
    #[error("message: {0}")] Message(#[from] MessageError),
    #[error("validation: {0}")] Validation(#[from] ValidationError),
    #[error("rate limit: {0}")] RateLimited(RateLimitInfo),
    #[error("not found")] NotFound,
    #[error("internal: {0}")] Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn code(&self) -> ErrorCode {
        // 与 aux-03 §B 一一映射;单一来源
        match self {
            AppError::Identity(IdentityError::Unauthenticated) => ErrorCode::Unauthorized,
            AppError::Identity(IdentityError::Banned) => ErrorCode::AccountBanned,
            AppError::Identity(IdentityError::Suspended) => ErrorCode::AccountSuspended,
            AppError::Conversation(ConversationError::NotFound) => ErrorCode::ConversationNotFound,
            AppError::Conversation(ConversationError::AlreadyExists { .. }) => ErrorCode::FriendRequestExists,
            AppError::Message(MessageError::NotFound) => ErrorCode::MessageNotFound,
            AppError::Message(MessageError::RecallWindowExpired) => ErrorCode::RecallWindowExpired,
            AppError::Message(MessageError::InvalidStateTransition { .. }) => ErrorCode::InvalidStateTransition,
            AppError::Message(MessageError::IdempotencyConflict) => ErrorCode::IdempotencyConflict,
            AppError::Message(MessageError::TooLarge) => ErrorCode::MessageTooLarge,
            AppError::Validation(_) => ErrorCode::ValidationError,
            AppError::RateLimited(_) => ErrorCode::RateLimited,
            AppError::NotFound => ErrorCode::NotFound,
            AppError::Internal(_) => ErrorCode::InternalError,
        }
    }
    pub fn http_status(&self) -> u16 { /* per aux-03 §B mapping table */ }
    pub fn grpc_code(&self) -> tonic::Code { /* per aux-13 §2.5 */ }
}
```

`AppError::code()` 是错误语义的**唯一来源**:`im-gateway` 边界统一转换为 HTTP 状态码 / WS `error.code` / gRPC `tonic::Code`,不在多处重复定义映射关系(保证 §10 单一原则)。

## 10. 配置项清单(环境变量,MVP)

> 配置分层、加载方式、密钥轮换见 `BasicDesign.md §14`,本节仅列具体变量。

### 10.1 必填启动配置(缺失则 panic 退出)

| 变量 | 说明 | 示例 |
|---|---|---|
| `IM_DATABASE_URL` | PostgreSQL 连接串 | `postgres://im:im@localhost:5432/im` |
| `IM_VALKEY_URL` | Valkey 连接串 | `redis://localhost:6379/0` |
| `IM_NATS_URL` | NATS 连接串 | `nats://localhost:4222` |
| `IM_MINIO_ENDPOINT` | MinIO endpoint | `http://minio:9000` |
| `IM_MINIO_ACCESS_KEY` | MinIO 凭证 | — |
| `IM_MINIO_SECRET_KEY` | MinIO 凭证 | — |
| `IM_JWT_SIGNING_KEYS` | Access Token 签名密钥,**JSON 数组格式** | `'[{"kid":"v1","key":"hex64..."},{"kid":"v2","key":"hex64..."}]'` |
| `IM_SERVER_SECRETS` | 各 Environment 的 server_secret,JSON map | `'{"env_id_1":"hex64...","env_id_2":"hex64..."}'` |
| `IM_REFRESH_TOKEN_PEPPER` | Refresh Token 哈希加盐 | `hex64...` |

### 10.2 有默认值的可调配置

| 变量 | 说明 | 默认 | Candidate? |
|---|---|---|---|
| `IM_HTTP_PORT` | HTTP 监听端口 | `8080` | N |
| `IM_GRPC_PORT` | im-gateway 暴露给 im-core 的端口(im-gateway 同时跑 HTTP + gRPC) | `9000` | N |
| `IM_ENV` | 运行环境 ∈ {`dev`,`staging`,`prod`} | `dev` | N |
| `IM_LOG_LEVEL` | tracing 级别 ∈ {`trace`,`debug`,`info`,`warn`,`error`} | `info` | N |
| `IM_ACCESS_TOKEN_TTL_SECONDS` | Access Token 有效期 | `900` (15min) | **Y** (BENCHMARK REQUIRED) |
| `IM_REFRESH_TOKEN_TTL_SECONDS` | Refresh Token 有效期 | `2592000` (30d) | **Y** |
| `IM_WS_HEARTBEAT_TIMEOUT_SECONDS` | WS 心跳超时 | `60` | N |
| `IM_WS_PING_INTERVAL_SECONDS` | 客户端建议 ping 间隔(SDK 文档暴露) | `30` | N |
| `IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN` | 单用户每分钟发送消息上限(可被 `environments.settings` 覆盖) | `60` | **Y** |
| `IM_RATE_LIMIT_GUEST_REGISTER_PER_HOUR` | Guest 注册按 IP 每小时上限 | `10` | N |
| `IM_MESSAGE_RECALL_WINDOW_SECONDS` | 撤回时间窗(可被 `environments.settings` 覆盖) | `120` | **Y** |
| `IM_MESSAGE_MAX_SIZE_BYTES` | 消息 content JSON 序列化最大字节 | `65536` (64KB) | **Y** |
| `IM_RATE_LIMIT_BUCKET_SIZE` | 令牌桶容量 | `100` | N |
| `IM_SETTINGS_REFRESH_INTERVAL_SECONDS` | environments.settings 缓存刷新间隔 | `300` | N |
| `IM_DB_POOL_MAX_CONNECTIONS` | sqlx 连接池上限 | `20` | N |
| `IM_NATS_RECONNECT_MAX_ATTEMPTS` | NATS 客户端重连最大尝试 | `60` | N |

### 10.3 可观测性相关

| 变量 | 说明 |
|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry Collector 地址(如 `http://otel-collector:4317`) |
| `OTEL_SERVICE_NAME` | 服务名(默认 = crate 名) |
| `IM_PROMETHEUS_BIND` | Prometheus exporter 绑定地址(如 `0.0.0.0:9100`),空 = 不启用 |

### 10.4 K3s Secret 注入

| Secret Key(在 `im-env-{environment_id}` Secret 中) | 用途 | 长度 |
|---|---|---|
| `server_secret` | 游戏服务器 HMAC 签名 | 32 字节 (hex) |
| `jwt_signing_key_v1` | 当前生效的 JWT 签名密钥 | 32 字节 (hex) |
| `jwt_signing_key_v2` | 轮换期的新密钥(轮换结束后置空) | 32 字节 (hex) |
| `refresh_token_pepper` | Refresh Token 哈希加盐 | 32 字节 (hex) |
| `database_url` | IM_DATABASE_URL 同值 | — |
| `valkey_url` | IM_VALKEY_URL 同值 | — |
| `nats_url` | IM_NATS_URL 同值 | — |
| `minio_endpoint` / `minio_access_key` / `minio_secret_key` | MinIO 凭证 | — |

所有 `Candidate` 标注的默认值需在 SRS PoC-02 / 压测阶段验证后固化,本文档不视为最终值;固化后从本表移除 Candidate 标记,同步 SRS §49 SLO 候选表。

## 11. 与上游文档的追溯关系

> 本表是 BasicDesign → DetailedDesign → ImplementationSpec 链条的逆向追溯,任一需求 ID 在下游实现中应至少有一处映射。

### 11.1 DetailedDesign → SRS 需求 ID 追溯

| 本文档章节 | 内容 | 对应 SRS 需求 ID | 对应 BasicDesign 章节 |
|---|---|---|---|
| §2 gRPC `CoreService` | 内部服务契约 | —(内部接口) | §2 / §3 |
| §3.1 WS 客户端帧 `auth` | 鉴权入口 | IM-ID-003, SEC-NFR-003 | §9 |
| §3.1 WS `send_message` | 发消息主路径 | IM-FR-001, IM-FR-002, IM-FR-003, IM-MSG-001 | §7 / §9 |
| §3.1 WS `edit_message` | 消息编辑 | IM-MSG-002 | §7 |
| §3.1 WS `recall_message` | 消息撤回 | IM-MSG-003 | §7 |
| §3.1 WS `react` | reaction | IM-MSG-002 | §7 |
| §3.1 WS `mark_read` | 已读回执 | IM-FR-004, IM-MSG-002 | §7 |
| §3.1 WS `typing` | typing 指示 | IM-PRES-002 | §7 |
| §3.1 WS `ping` | 心跳 | NET-FR-001 | §11 |
| §3.2 WS `message_new` | 实时推送 | IM-FR-001, IM-FR-005 | §7 |
| §3.2 WS `message_edited` / `message_recalled` / `reaction_added` | 事件广播 | IM-MSG-002/003 | §7 |
| §3.2 WS `presence_update` | 在线状态 | IM-PRES-001, IM-PRES-002, IM-PRES-004 | §9 |
| §3.2 WS `force_disconnect` | 强制下线 | IM-ID-002 (single logout), SEC-NFR-006 | §9 |
| §4 Message content JSON Schema | 6 种 kind 字段定义 | IM-MSG-001 | §4 |
| §4 Conversation.metadata 命名空间 | `game.*` / `ai.*` / `work.*` 扩展 | IM-CONV-002, GAME-SOC-001 | §4 |
| §5 `POST /v1/auth/token/exchange` | Server-to-Server Token 兑换(红线) | GAME-ID-003, SEC-NFR-005 | §6 |
| §5 `POST /v1/auth/guest` | Guest 临时身份 | IM-ID-001, GAME-ID-002 | §6 |
| §5 `POST /v1/auth/refresh` | Token 旋转 | IM-ID-003, SEC-NFR-003 | §6 |
| §5 `POST /v1/auth/link` | Guest Upgrade + 合并冲突 | IM-ID-005, GAME-ID-004 | §6 |
| §5 `POST /v1/auth/logout` | 撤销设备会话 | IM-ID-002 | §6 |
| §5 `POST /v1/conversations` | 创建 DM/Group/Channel | IM-CONV-001, IM-CONV-002 | §4 |
| §5 `GET /v1/conversations` | 会话列表 | IM-CONV-001 | §4 |
| §5 `GET /v1/conversations/{id}/messages?after_sequence=` | 离线增量同步(核心) | IM-FR-005, NET-FR-001 | §7 |
| §5 `POST /v1/conversations/{id}/messages` | REST 发消息(SDK fallback) | IM-FR-002/003, IM-MSG-001 | §7 |
| §5 `PATCH /v1/conversations/{id}/messages/{msg_id}` | 编辑消息 | IM-MSG-002 | §7 |
| §5 `POST /v1/conversations/{id}/messages/{msg_id}/recall` | 撤回 | IM-MSG-003 | §7 |
| §5 `POST /v1/conversations/{id}/messages/{msg_id}/reactions` | reaction | IM-MSG-002 | §7 |
| §5 `POST /v1/conversations/{id}/read` | 已读回执 | IM-FR-004 | §7 |
| §5 `POST /v1/friends/requests` | 好友申请 | IM-REL-001 | §4 |
| §5 `POST /v1/friends/requests/{id}/respond` | 接受/拒绝 | IM-REL-001 | §4 |
| §5 `POST /v1/friends/{id}/block` | 拉黑 | IM-REL-001, MOD-FR-001 | §4 |
| §5 `GET /v1/friends` | 好友列表 | IM-REL-001 | §4 |
| §5 `POST /v1/media/presign` | 预签名 URL 上传 | IM-MEDIA-001, IM-MEDIA-002 | §10 |
| §5 `GET /v1/media/{id}` | 媒体可读 URL | IM-MEDIA-001 | §10 |
| §5 `GET /v1/me` / `PATCH /v1/me` | 用户资料 | IM-ID-001 | §4 |
| §5 `GET /healthz` / `/readyz` | 健康检查 | OPS-NFR-001, OBS-REQ-* | §15 |
| §5 `GET /metrics` | Prometheus 指标 | OBS-MET-* | §15 |
| §6.1 Message.state 状态机 | sent → delivered → read / 旁路 recalled/deleted | IM-FR-004 | §5 |
| §6.2 User.state 状态机 | active ↔ banned/suspended → deleted | IM-ID-004, SEC-NFR-006 | §4 |
| §6.3 DeviceSession 生命周期 | created → rotated / revoked | IM-ID-002/003, SEC-NFR-003 | §6 |
| §7 错误码表(20 项) | 单一错误来源,全协议贯穿 | IM-FR-003 (IDEMPOTENCY), SEC-NFR-004 (RATE_LIMITED), 全局 | §5 / §9 |
| §8 DB 迁移文件骨架 | 6 个迁移,模块化命名 | — | §4 |
| §9.1 MessageRepository trait | 主路径抽象 | IM-FR-002, IM-FR-003 | §3 |
| §9.1 MessageService.send_message | 幂等+Sequence+事件三步 | IM-FR-001/002/003 | §3 / §5 |
| §9.2 TokenService | Access + Refresh 签发/校验/旋转 | IM-ID-003, SEC-NFR-003 | §6 / §14 |
| §9.2 UserRepository | User CRUD + state 变更 | IM-ID-001/004 | §4 / §6 |
| §9.3 ConversationService | DM/Group 创建+成员管理 | IM-CONV-001, IM-REL-002 | §4 |
| §9.4 WsSession | WS 帧循环 + push | IM-FR-001, IM-PRES-* | §9 |
| §9.5 AppError 统一映射 | 错误单一来源 → aux-03 | 全局 | §5 / §9 |
| §10.1 必填配置 9 项 | 启动校验 | OPS-NFR-* | §14 |
| §10.2 可调配置 14 项 | 含 4 项 Candidate | (需压测固化) | §14 |
| §10.4 K3s Secret 注入 | 密钥管理 | SEC-NFR-003, IM-ID-003 | §14 |

### 11.2 aux 文档 → DetailedDesign 追溯

| aux 文档 | 内容 | 对应 DetailedDesign 章节 |
|---|---|---|
| `aux-01-naming-convention.md` | 命名规范(Rust/TS/DB/API/协议) | §2/§3/§5 字段命名,全文档 |
| `aux-02-data-dictionary.md` | 14 张表字段详细属性 | §8, BasicDesign §4 |
| `aux-03-error-code-registry.md` | 20 项错误码 + HTTP/gRPC 映射 | §7, §9.5 |
| `aux-04-state-machine-spec.md` | 业务对象状态机 | §6.1/§6.2/§6.3 |
| `aux-05-crc-card.md` | CRC 卡片(Class-Responsibility-Collaborator) | §9 trait 拆解 |
| `aux-06-algorithm-performance-model.md` | 关键算法复杂度/压测模型 | §5 (Sequence), §9.1 (find_by_idempotency_key 索引命中) |
| `aux-07-sql-optimization-checklist.md` | SQL 优化 + Core Schema 纯净性 | §8, BasicDesign §4 |
| `aux-08-batch-retry-dlq.md` | 批处理重试 + 死信队列 | §9.1 (事件 publish 失败 outbox) |
| `aux-09-log-query-cookbook.md` | 日志查询 Cookbook | §10, Observability §1-3 |
| `aux-10-dd-review-detailed-checklist.md` | DD Review Checklist | (评审时使用) |
| `aux-11-key-sequence-diagrams.md` | 关键时序图集(mermaid) | §3, §5, §9.1, §9.4 |
| `aux-12-config-spec.md` | 配置项规格 | §10, BasicDesign §14 |
| `aux-13-protocol-frame-samples.md` | 协议帧样例 | §2/§3/§5 |

### 11.3 下游交付物 → DetailedDesign 追溯

| 下游 | 引用章节 |
|---|---|
| `ImplementationSpec.md`(实施规范) | 全部章节 |
| `crates/im-proto/proto/*.proto` | §2 |
| `crates/im-protocol/src/ws_frames.rs` | §3 |
| `crates/im-gateway/src/api/*.rs` | §5 |
| `migrations/0001-0006*.sql` | §8 |
| `crates/im-core/src/**/{repository,service}.rs` | §9 |
| `crates/im-common/src/error.rs` | §9.5 |
| `k8s/{dev,staging,prod}/secrets.yaml`(模板) | §10.4 |
| `docs/aux-13` 协议样例 | §3/§5 |
| `docs/aux-03` 错误码 | §7/§9.5 |
| `Observability.md` SLO 指标 | §10.3 |

## 12. 遗留待确认项（编码阶段前需闭环，非文档完整性阻塞项）

- WS帧序列化格式（JSON vs Protobuf二进制）：ADR-014，Candidate暂定JSON。
- sqlx迁移双向脚本 vs 单向脚本约定：交由实现时选定的工具链版本决定。
- Rate Limit / Token TTL / 撤回时间窗的具体数值:均为Candidate,压测与产品确认后回填。

---

## X. Bevy 客户端 ECS 模块设计(草案,待 DDD Review)

> **本节性质**:`v0.2` 新增章节,全部内容为草案,待 DDD Review 拍板。`v0.1 Draft` §1-§12 既有内容**不**改动;WS 协议细节(§3)、gRPC 契约(§2)、REST API(§5)由 v0.1 Draft 已覆盖,本节**不**重复展开。
> **范围边界**:仅定义 Bevy ECS 侧的 Component / System / Resource 草案与文件结构草案。实际新增 `crates/bevy-client` crate 留 v0.3+ 实施(`Cargo.toml` workspace 当前不含此 crate,本节不修改 `Cargo.toml`)。Physis 物理引擎整合**不**包含,留 v0.3+ 议。

### X.1 ECS Component 草案

```rust
// 草案,字段命名/类型/数量待 DDD Review 拍板
use bevy::prelude::*;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Component)]
pub struct AgentId(pub u64);

#[derive(Component)]
pub struct AgentStatus {
    pub online: bool,
    pub last_active: Instant,
}

#[derive(Component)]
pub struct AgentPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Component, Default)]
pub struct MessageBuffer {
    pub messages: VecDeque<ImMessage>,
}

// ImMessage 类型定义:草案
// 真实结构应与 im-gateway WS §3.2 `message_new` 帧 `message` 字段对齐,
// v0.2 暂以占位类型表达,字段展开留 v0.3+ 实施。
#[derive(Debug, Clone)]
pub struct ImMessage {
    pub id: String,
    pub conversation_id: String,
    pub sequence: i64,
    pub sender_id: Option<String>,
    pub kind: String,
    pub content: serde_json::Value,
    pub created_at: String,
}
```

**草案待 DDD Review 拍板项**:
- `AgentId(u64)` vs `AgentId(String)`(string 形式更便于与 im-core UUID 对齐,u64 内存更紧凑)——待 DDD Review 拍板
- `AgentPosition` 仅 2D 坐标,是否需要 z / rotation / scale 表达 3D 场景?——待 DDD Review 拍板
- `MessageBuffer.messages` 是否分桶(已读 / 未读 / 系统消息)?——待 DDD Review 拍板
- `AgentStatus.last_active` 持久化策略(Valkey 缓存 vs 内存)?——待 DDD Review 拍板

### X.2 ECS System 草案

| System | 输入 | 输出 | 触发时机 | 草案备注 |
|---|---|---|---|---|
| `agent_spawn_system` | `WsConnection`(WS 收到 agent 列表帧) | 创建 Entity + 挂 `AgentId` / `AgentStatus` / `AgentPosition` / `MessageBuffer` | WS 收到 `agent_list` 帧(协议细节见 §3,本节**不**展开) | 草案,实际可能拆为 `agent_spawn_from_initial` + `agent_spawn_from_incremental` |
| `agent_message_system` | `WsConnection`(WS `message_new` 帧) + `AgentRegistry` | 找到目标 agent Entity,`MessageBuffer.push` | 每次 WS `message_new` 帧到达 | 草案,反序列化与错误处理需 v0.3+ 细化 |
| `agent_render_system` | 所有含 `AgentPosition` + `AgentStatus` 的 Entity | 渲染输出(Bevy 2D sprite / 3D mesh,草案) | 每帧,FixedPostUpdate 或 Update stage | 草案,具体渲染管线(Bevy 2D vs 3D vs 自定义)留 v0.3+ 拍板 |
| `ui_chat_panel_system` | `UiState.selected_agent_id` + `AgentRegistry` + 各 Entity 的 `MessageBuffer` | 渲染左侧 agent 列表 + 中间消息流(早期飞书风格) | 每帧,Update stage | 草案,UI 框架选择(bevy_egui / 自定义 Text2d / bevy_ui)留 v0.3+ 拍板 |
| `ws_heartbeat_system` | `WsConnection` | 每 30s 发送 `{ "type": "ping", "ts": ... }` | Timer,FixedUpdate 1s tick | 草案,30s 间隔与 §3 WS 协议一致;具体 timer 资源 vs Timer Component 待 v0.3+ 细化 |

**草案待 DDD Review 拍板项**:
- System 切片粒度(草案 5 个,实际可能拆分更细,例如 `agent_message_system` 拆为 deserialize / route / append 三段)——待 DDD Review 拍板
- `agent_render_system` 用 Bevy 内置 2D 渲染 vs 自定义 WGSL shader——待 DDD Review 拍板
- `ui_chat_panel_system` 选型(bevy_egui 快速原型 vs 自定义 Text2d 精细控制)——待 DDD Review 拍板

### X.3 ECS Resource 草案

```rust
// 草案,与 §3 WS 协议对接细节待 v0.3+ 实施
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;  // 草案,具体通道库待 v0.3+ 拍板

#[derive(Resource)]
pub struct WsConnection {
    // 草案:WS 客户端抽象(具体实现用 tokio-tungstenite vs actix-ws vs 其他 待 v0.3+ 拍板)
    pub inbound_rx: mpsc::UnboundedReceiver<WsFrame>,
    pub outbound_tx: mpsc::UnboundedSender<WsFrame>,
    // 连接状态机:草案(Connecting / Authenticating / Open / Reconnecting / Closed)
    pub state: WsConnState,
}

#[derive(Resource, Default)]
pub struct AgentRegistry {
    pub agent_id_to_entity: HashMap<u64, Entity>,
    // 草案:是否需要反向索引 entity -> agent_id?——待 DDD Review 拍板
}

#[derive(Resource, Default)]
pub struct UiState {
    pub selected_agent_id: Option<u64>,
    // 草案:会话状态(打开的 chat panel / 输入框草稿 / 滚动位置)——待 DDD Review 拍板
    pub conversation_state: HashMap<u64, ConversationPanelState>,
}

#[derive(Debug, Clone, Default)]
pub struct ConversationPanelState {
    pub draft: String,
    pub scroll_offset: f32,
}

// WsFrame:占位类型,实际定义应对齐 §3.1/§3.2 帧结构,
// v0.2 不展开,留 v0.3+ 实施。
#[derive(Debug, Clone)]
pub struct WsFrame(pub serde_json::Value);

#[derive(Debug, Clone)]
pub enum WsConnState {
    Connecting,
    Authenticating,
    Open,
    Reconnecting { attempt: u32 },
    Closed,
}
```

**草案待 DDD Review 拍板项**:
- `WsConnection` 用 tokio-tungstenite(草案倾向)还是 actix-ws(与 §9.4 `im-gateway` 端 actix_ws 对称)?——待 DDD Review 拍板
- `AgentRegistry` 是否独立 Resource 还是挂 World-level metadata?——待 DDD Review 拍板
- `UiState.conversation_state` HashMap 选型 vs LRU cache(1M agent 内存预算)——待 DDD Review 拍板

### X.4 性能预算(草案,Bevy 默认 2D 渲染,实测数据待 DDD Review 拍板)

> **未经实测,所有数字为草案,仅作 v0.3+ 实施时的目标参考。** 实际压测数据需在 v0.3+ 实施后回填,届时同步 SRS §49 SLO 候选表。

**目标规模**:1M agent entity @ 60 FPS(每帧 16.67ms 预算)

| 子系统 | 单帧预算(草案) | 草案备注 |
|---|---|---|
| spawn / despawn(`agent_spawn_system` 等) | 0.5ms | 草案,实际可能按需 batch |
| 消息分发(`agent_message_system`) | 0.5ms | 草案,HashMap 路由 O(1) |
| 渲染(`agent_render_system`) | 8ms | 草案,基于 Bevy 默认 2D 渲染管线的常见比例 |
| UI(`ui_chat_panel_system`) | 2ms | 草案,假设只渲染当前选中 agent 的消息流 |
| 心跳(`ws_heartbeat_system`) | 0.1ms | 草案,Timer-only 系统,极轻量 |
| **合计** | **11.1ms / 16.67ms** | 草案,留 5.57ms 余量给 GC / 调度 / WS I/O |

**内存预算**(草案):
- 单 agent Component 内存占用 ≈ 200 字节(`AgentId` 8B + `AgentStatus` ~32B + `AgentPosition` 8B + `MessageBuffer` ~152B 含 10 条消息缓冲)
- 1M agent × 200B = 200 MB(远低于 SRS 1 GB 预算,草案,留余量给 system overhead / 资源加载)

**草案待 DDD Review 拍板项**:
- 16.67ms 60 FPS 目标 vs 30 FPS(游戏客户端常见 30 FPS 更稳)?——待 DDD Review 拍板
- 200 字节/agent 估算(实际取决于 `MessageBuffer` 容量与 `ImMessage` 字段大小)?——待 DDD Review 拍板
- 8ms render 预算(Bevy 2D vs 3D 差距大)?——待 DDD Review 拍板

### X.5 文件结构草案

> **本节为 v0.2 文档草案定义,实际新增 `crates/bevy-client` crate 留 v0.3+ 实施**。当前 8 crate workspace 不含此 crate,本节**不**修改 `Cargo.toml`。

```
crates/bevy-client/                    # 草案路径,留 v0.3+ 实施
├── src/
│   ├── main.rs                        # Bevy App 入口
│   ├── components/                    # Component 定义(见 X.1)
│   ├── systems/                       # System 实现(见 X.2)
│   ├── resources/                     # Resource 定义(见 X.3)
│   ├── ui/                            # 早期飞书风格 UI(草案)
│   └── ws/                            # tokio-tungstenite 客户端(草案,见 X.3 拍板项)
├── Cargo.toml                         # 草案,workspace member 留 v0.3+ 实施
└── tests/                             # 草案,集成测试留 v0.3+ 落地
```

**草案待 DDD Review 拍板项**:
- `crates/bevy-client` 路径与命名(是否拆为 `bevy-client` + `bevy-client-ui` 两 crate)?——待 DDD Review 拍板
- 与 im-gateway WS 协议对接细节(草案参考 §3,客户端实现留 v0.3+)——已知缺口
- 早期飞书风格 UI 的具体表现(颜色 / 字体 / 间距 / 头像布局)?——待 DDD Review 拍板

### X.6 已知缺口(必查项,DDD Review 必看)

> 以下项均为 v0.2 显式未覆盖的范围,缺标比错标安全——**不**自行脑补,留 DDD Review 拍板或 v0.3+ 实施。

1. **Component 字段草案**:`AgentId` / `AgentStatus` / `AgentPosition` / `MessageBuffer` 字段为草案,未与 im-gateway 真实协议对齐,DDD Review 必查。
2. **System 切片草案**:5 个 System 粒度为草案,实际可能拆分更细(例如 `agent_message_system` 拆 deserialize / route / append)。
3. **性能数字草案**:16.67ms / 200 MB / 8ms render 全部未经实测,仅基于 Bevy 默认 2D 渲染 + 1M entity 粗算;实测数据需 v0.3+ 实施后回填。
4. **`crates/bevy-client` 路径草案**:当前 8 crate workspace 不含此 crate,**v0.2 仅在文档定义**,实际新增 crate 留 v0.3+ 实施。本节**不**修改 `Cargo.toml`。
5. **与 im-gateway WS 协议对接细节**:§3 v0.1 Draft 已覆盖完整 WS 帧定义;**Bevy 客户端侧**的反序列化、错误处理、重连策略 v0.2 暂以 `WsFrame(serde_json::Value)` 占位,实际实现留 v0.3+。
6. **Physis 物理引擎整合**:**不**包含在 v0.2 范围,Physis 整合方案留 v0.3+ 议(避免在 ECS 草案未定型时混入物理集成复杂度)。
7. **UI 框架选型**:bevy_egui / 自定义 Text2d / bevy_ui 三选一,草案倾向 bevy_egui(快速原型),DDD Review 必查。
8. **WS 客户端库选型**:tokio-tungstenite(草案倾向) vs actix-ws,需考虑与 §9.4 `im-gateway` 端 actix_ws 的对称性,DDD Review 必查。
9. **1M entity 渲染管线**:Bevy 默认 2D 渲染 vs 自定义 WGSL shader vs Bevy 3D 场景图,DDD Review 必查。
10. **错误处理边界**:ECS System 内 WS 错误 / 反序列化错误的处理策略(回滚 Entity / 标记 dirty / 静默丢弃 + 日志),草案未覆盖,DDD Review 必查。

### X.7 与上游文档的追溯关系(草案)

> 草案追溯表,非最终值,DDD Review 拍板后固化。

| 本节章节 | 内容 | 对应 SRS 需求 ID(草案) | 对应 BasicDesign 章节(草案) | 对应 v0.1 Draft 章节 |
|---|---|---|---|---|
| X.1 ECS Component | agent / message 状态表达 | (新增,需求 ID 待 DDD Review 分配) | §12(SDK 客户端架构,草案) | §3.2 `message_new` 帧,§4 Message JSON Schema |
| X.2 ECS System | spawn / message / render / UI / heartbeat | IM-FR-001(实时消息), IM-PRES-002(typing), NET-FR-001(心跳) | §12 | §3.1 客户端帧(ping/send_message 等),§9.4 WsSession |
| X.3 ECS Resource | WsConnection / AgentRegistry / UiState | IM-FR-005(增量同步,与 AgentRegistry 对接), NET-FR-001(WS 连接) | §12 | §3 WS 协议,§9.4 WsSession |
| X.4 性能预算 | 1M entity @ 60 FPS | (新增,SLO 需求 ID 待 DDD Review 分配) | §15 Observability | §10.3 可观测性配置 |
| X.5 文件结构 | `crates/bevy-client` | — | §12 | §1 代码仓库结构(v0.1 Draft 8 crate 不含此 crate,v0.2 草案新增) |

**v0.2 修订历史**:
- 2026-08-27:Mavis 接手 agent per DEC-008 新增 §X Bevy 客户端 ECS 模块设计草案(只增不删 v0.1 Draft §1-§12)。修订者:Mavis 接手 agent per DEC-008。

