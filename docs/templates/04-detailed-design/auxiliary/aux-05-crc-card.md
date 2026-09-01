---
doc_id: aux-05
title_ja: CRC カード (IM1.0)
title_zh: CRC 卡 (IM1.0) - IM1.0 核心类职责卡
phase: 04-detailed-design-aux
owners: 架构师 (Mavis 接手 agent per DEC-008) + Tech Lead
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 44 类设计, 43 模块设计, 45 逻辑设计
---

# aux-05. CRC カード (IM1.0) / CRC 卡 (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: 架构师 + Tech Lead
> 类源: `docs/ImplementationSpec.md` §7.4 (im-core 5 大模块完整 trait 清单)
> 字段字典: `aux-02-data-dictionary.md` §F.4 / §F.6 / §F.8 / §F.12
> 协议: `aux-13-protocol-frame-samples.md`(帧 JSON 结构 + 错误码)

## 1. 目的 (Purpose)

为 IM1.0 核心类(User / Conversation / Message / FriendRequest 4 张 CRC)提供简明职责卡,聚焦"做什么 / 不做什么 / 跟谁协作",作为代码评审的快速参照。本表覆盖 MVP 阶段 4 大核心聚合根,其他类(UserState / Friendship 等)留 V1+ 补充。

## 2. 适用范围 (Scope)

| 类 | 类型 | 所在 crate | 模块路径 |
|---|---|---|---|
| `User` | 聚合根 | `im-core` | `identity/user.rs` |
| `Conversation` | 聚合根 | `im-core` | `conversation/conversation.rs` |
| `Message` | 实体 | `im-core` | `message/message.rs` |
| `FriendRequest` | 实体 | `im-core` | `relationship/friend_request.rs` |

> 不在范围: 值对象(`MessageContent` / `MessageKind` / `ExternalIdentity` 等)留 V1+ 单独 CRC;服务层(`IdentityService` / `ConversationService` 等)在 §G 补充"服务层 CRC 索引"。

## 3. 责任方 (Owners)

架构师(定义 + 评审)+ Tech Lead(编码 + 评审)。任何 CRC 变更需 Tech Lead 同意。

## 4. 前置依赖 (Prerequisites / Inputs)

- 模块设计(`ImplementationSpec §7.4`)
- 数据字典(`aux-02 §F.4 / §F.6 / §F.8 / §F.12`)
- 状态机(`aux-04 §B` 4 大状态机)
- 错误码(`aux-03 §B` 21 项)
- 协议帧(`aux-13 §1` 12 类 WS 帧 + 错误响应体)

## 5. 输出 / 模板正文 (Body)

---

### CRC-001 User

| 维度 | 内容 |
|---|---|
| 所在模块 | `crates/im-core/src/identity/` |
| 类型 | 聚合根(Aggregate Root) |
| 关键字段 | `id` (UUID), `environment_id` (UUID), `kind` (user/guest), `state` (active/banned/suspended/deleted), `external_identity` (JSONB?), `display_name` (TEXT?), `created_at` |
| 状态机 | `aux-04 §B.1`(4 状态,`deleted` 终态) |
| 隐私 | 内部 ID / 公开 display_name / 敏感 external_identity / 不可见 password / refresh_token_hash |

**职责 (Responsibilities)**

- [ ] 持有用户身份(由 `kind=user` 持 external_identity;`kind=guest` 持 NULL)
- [ ] 维护 `state` 状态机(只允许经 `try_transition` 转换,见 `aux-04 §B.1`)
- [ ] 提供 `is_writable() -> bool`(`state == Active || Suspended` 中 Suspended 仅部分写,实际 MVP 仅 Active 可写)
- [ ] 提供 `can_create_dm() -> bool`(Guest 受限,仅 dm;`ImplementationSpec §3.1.2`)
- [ ] 写 `audit_logs` 当 `state_changed`(`aux-04 §C.3` 强制)

**协作者 (Collaborators)**

- → `IdentityService`: 创建 / 状态变更 / 资料更新
- → `UserRepository`: 持久化 + 按 id / external_identity 查询
- → `DeviceSession`: 关联 0..N 个 device session(1:N)
- → `Conversation`(作为 member): 关联 0..N 个 conversation(通过 `conversation_members`)
- → `Message`(作为 sender): 关联 0..N 条消息(1:N)
- ← `TokenService`: 签发 access / refresh token 后绑定 user_id
- ← `AuthMiddleware`: 校验 token claims 中的 user_id,触发 load

**不在职责范围内 (Not responsible for)**

- ❌ 不负责密码哈希(`PasswordHasher` 负责,见 `ImplementationSpec §7.4.1`)
- ❌ 不负责 token 签发 / 校验(`TokenService` 负责)
- ❌ 不负责设备管理(`DeviceSession` 独立聚合,见 `aux-02 §F.5`)
- ❌ 不负责好友关系状态(`Friendship` 独立聚合,见 `aux-04 §B.3`)
- ❌ 不负责消息发送(`MessageService` 负责)

**关键方法 (Key Methods)**

| 方法 | 入参 | 出参 | 复杂度 | 备注 |
|---|---|---|---|---|
| `new(env, kind, ext_id?, display_name?)` | 构造参数 | `Self` | O(1) | INSERT users,触发 state='active' |
| `try_transition(event)` | `UserEvent` | `Result<Self, TransitionError>` | O(1) | 见 `aux-04 §E.1` trait 草案 |
| `is_writable()` | — | `bool` | O(1) | 简化:state == Active |
| `can_create_dm()` | — | `bool` | O(1) | Guest 仅 dm,非 guest 任意 kind |
| `update_display_name(name?)` | `Option<&str>` | `Self` | O(1) | 走 `IdentityService::update_me` |
| `ban(actor)` / `unban(actor)` / `suspend(actor)` / `resume(actor)` / `delete(actor)` | admin actor | `Self` | O(1) | 走 `IdentityService::update_state` + audit |

**测试要点 (Test Points)**

- 正常: 注册新 user → state=active + 写 audit
- 正常: banned user 查询 → 返回 row 但 is_writable=false
- 正常: deleted user 任何写 → 404 USER_NOT_FOUND
- 异常: 非法 state 转换(例如 active → recalled 等不存在转换)→ `TransitionError::Invalid`
- 异常: 自传 `state='unknown'` → DB CHECK 约束拒绝(SQL 层)
- 边界: Guest 试图创建 group 会话 → can_create_dm=false → API 层拒绝
- 边界: external_identity 为 NULL 但 kind=user → 服务层校验拒绝(应用层)
- 边界: 跨 env 试图访问 user → FK 阻断(应用层校验)
- 并发: 1000 并发 `update_state` → 乐观锁/串行化保证不丢更新(V1+ 评估, MVP 用 `try_transition` 串行)

**示例签名**

```rust
// crates/im-core/src/identity/user.rs(待 C-1 实装)
#[derive(Debug, Clone)]
pub struct User {
    pub id: UserId,
    pub environment_id: EnvironmentId,
    pub kind: UserKind,                   // enum: User | Guest
    pub state: UserState,                 // enum: Active | Banned | Suspended | Deleted
    pub external_identity: Option<ExternalIdentity>,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn try_transition(self, event: UserEvent) -> Result<Self, TransitionError> { /* ... */ }
    pub fn is_writable(&self) -> bool { matches!(self.state, UserState::Active) }
    pub fn can_create_dm(&self) -> bool { /* 总是 true;can_create_group 由 can_create_dm 扩展 */ }
}
```

---

### CRC-002 Conversation

| 维度 | 内容 |
|---|---|
| 所在模块 | `crates/im-core/src/conversation/` |
| 类型 | 聚合根(Aggregate Root) |
| 关键字段 | `id` (UUID), `environment_id` (UUID), `kind` (dm/group/channel/system/broadcast), `metadata` (JSONB, 仅 `game.*`/`ai.*`/`work.*` 命名空间), `created_at` |
| 关联表 | `conversation_sequences` (1:1) / `dm_pairs` (0..1, 仅 dm) / `conversation_members` (1:N) |
| 生命周期 | `aux-04 §B.5`(无显式 state,通过 kind + 关联表推断) |

**职责 (Responsibilities)**

- [ ] 持有会话身份(由 `kind` 决定形态)
- [ ] 校验 `metadata` JSONB 命名空间(`aux-01 §I` 禁游戏专有字段,只允许 `game.*`/`ai.*`/`work.*`)
- [ ] 提供 DM 幂等创建: `find_or_create_dm(env, user_a, user_b) -> Self`(`ImplementationSpec §3.1.2`)
- [ ] 提供 `is_member(user_id) -> bool` (查 `conversation_members` 关联表)
- [ ] 提供 `last_sequence() -> i64`(查 `conversation_sequences.next_sequence - 1`)
- [ ] DM 私聊强制 `dm_pairs` 唯一对,`CHECK (user_a < user_b)` 规范化

**协作者 (Collaborators)**

- → `ConversationService`: 创建 / 列表 / 详情
- → `ConversationRepository`: 持久化
- → `ConversationMember`(作为关联实体): 0..N 个成员
- → `DmPair`(作为关联实体): 0..1 个 dm pair(仅 dm 类型)
- → `ConversationSequence`(作为关联实体): 1 个 sequence 分配器
- → `Message`: 关联 0..N 条消息
- ← `MessageService`: 发送消息前调 `is_member`
- ← `User`: 作为 member 关联

**不在职责范围内 (Not responsible for)**

- ❌ 不负责消息存储(`Message` 独立聚合)
- ❌ 不负责成员加入/离开的权限决策(由 `ConversationService` 调用 `IdentityService::is_writable` 决策)
- ❌ 不负责 sequence 分配(由 `SequenceAllocator` 负责,见 `ImplementationSpec §7.4.3`)
- ❌ 不负责 metadata 业务语义解析(Core 不解释 `game.*`,由 Extension / SDK 读)
- ❌ 不负责会话"软删除"策略(MVP 走 CASCADE 全删 + audit,见 `aux-04 §B.5`)

**关键方法 (Key Methods)**

| 方法 | 入参 | 出参 | 复杂度 | 备注 |
|---|---|---|---|---|
| `new(env, kind, metadata)` | 构造参数 | `Self` | O(1) | INSERT conversations + 1 个 sequence 分配器 |
| `find_or_create_dm(env, user_a, user_b)` | 双方 ID | `Self` | O(1) | 走 `dm_pairs` PK 幂等 |
| `is_member(user_id)` | `UserId` | `bool` | O(1) | 走 partial index 查询 |
| `add_member(user_id, role)` | 成员 + 角色 | `()` | O(1) | INSERT conversation_members |
| `remove_member(user_id)` | `UserId` | `()` | O(1) | DELETE conversation_members |
| `list_members()` | — | `Vec<ConversationMember>` | O(N) | 用于"会话成员"端点 |
| `last_sequence()` | — | `i64` | O(1) | 查 conversation_sequences |
| `validate_metadata(metadata)` | JSONB 值 | `Result<(), ValidationError>` | O(n) | 命名空间校验 |

**测试要点 (Test Points)**

- 正常: 创建 DM → 2 个 conversation_members 行(双方)
- 正常: 同对 DM 100 次并发创建 → 1 个 conversation(`dm_pairs` PK 阻断)
- 正常: 列表 "我的会话" → `WHERE user_id = $1` 走 `idx_conversation_members_user_id`(`migrations/0004` 第 51 行)
- 正常: Group 会话 add 成员 → 0..N members
- 正常: 频道(channel)尝试发消息 → API 拒绝(应用层校验)
- 异常: metadata 含 `guild_id`(无命名空间前缀) → 拒绝
- 异常: user 不在 conversation_members → is_member=false → API 403 FORBIDDEN
- 异常: dm 类型却 metadata 包含 group.* 命名空间 → 拒绝(命名空间错误)
- 边界: 空会话(0 成员)创建 → DB CHECK / 业务校验
- 边界: 1000 成员的 Group → is_member 查询走 partial index
- 并发: 1000 并发同对 DM 创建 → 1 个 conversation + 100 次返回相同 id

**示例签名**

```rust
// crates/im-core/src/conversation/conversation.rs(待 C-8 实装)
#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: ConversationId,
    pub environment_id: EnvironmentId,
    pub kind: ConversationKind,           // enum: Dm | Group | Channel | System | Broadcast
    pub metadata: serde_json::Value,      // 命名空间限制
    pub created_at: DateTime<Utc>,
}

impl Conversation {
    pub async fn find_or_create_dm(repo: &dyn ConversationRepository, env: EnvironmentId, user_a: UserId, user_b: UserId) -> Result<Self, ConversationError> { /* ... */ }
    pub async fn is_member(&self, repo: &dyn ConversationRepository, user: UserId) -> Result<bool, ConversationError> { /* ... */ }
    pub fn validate_metadata(metadata: &serde_json::Value) -> Result<(), ValidationError> { /* 命名空间 */ }
}
```

---

### CRC-003 Message

| 维度 | 内容 |
|---|---|
| 所在模块 | `crates/im-core/src/message/` |
| 类型 | 实体(Entity, 由 Conversation 聚合管理) |
| 关键字段 | `id` (UUID), `conversation_id` (UUID), `sequence` (BIGINT, 会话内单调), `sender_id` (UUID?, NULL=系统), `kind` (text/image/file/sticker/system/custom), `content` (JSONB), `reply_to` (UUID?, self-FK), `idempotency_key` (TEXT, max 128, UUID 形态), `state` (sent/delivered/read/recalled/deleted), `created_at`, `edited_at` (TIMESTAMPTZ?) |
| 状态机 | `aux-04 §B.4`(5 状态,`recalled`/`deleted` 终态) |
| 核心约束 | `UNIQUE (conversation_id, sequence)` + `UNIQUE NULLS NOT DISTINCT (conversation_id, sender_id, idempotency_key)`(`migrations/0005` 第 25/27 行) |

**职责 (Responsibilities)**

- [ ] 持有消息身份 + 内容
- [ ] 校验 `content` JSONB schema(按 `kind` 区分,见 `ImplementationSpec §3.1.3` + `aux-13 §4.1`)
- [ ] 校验 `idempotency_key` 合法(UUID 形态,max 128 字符)
- [ ] 维护 `state` 状态机(只允许经 `try_transition` 转换,见 `aux-04 §B.4`)
- [ ] 提供 `try_send_message` 5 步流程(成员校验 → 幂等键查重 → 分配 sequence → INSERT → fanout)
- [ ] 写 `audit_logs` 当 `state` 变更(`recalled`/`deleted` 必带,见 `aux-04 §C.3`)

**协作者 (Collaborators)**

- → `MessageService`: send / edit / recall / react / mark_read
- → `MessageRepository`: 持久化
- → `SequenceAllocator`: 分配 `sequence`(行锁,见 `ImplementationSpec §4.5`)
- → `Conversation`: 作为聚合根
- → `User`(作为 sender, 可为 NULL=系统消息)
- → `Message`(作为 reply_to, self-FK SET NULL)
- → `MessageReaction`(作为关联实体): 0..N 个 reaction
- ← `WsSession`: 接收 `send_message` 帧后调 `MessageService`
- ← `NatsEventPublisher`: publish `im.message.created/edited/recalled/reaction_added` 事件

**不在职责范围内 (Not responsible for)**

- ❌ 不负责 sequence 分配算法(`SequenceAllocator` 负责,见 `ImplementationSpec §7.4.3`)
- ❌ 不负责 WS 推送(`WsSession` + NATS fanout 负责)
- ❌ 不负责 content 业务语义(Core 仅校验 schema,不解释 `text` 字符串)
- ❌ 不负责已读回执的"全员已读"判断(由 `mark_read` + 客户端统计)
- ❌ 不负责离线推送(由 im-presence / im-media 负责,V1+)

**关键方法 (Key Methods)**

| 方法 | 入参 | 出参 | 复杂度 | 备注 |
|---|---|---|---|---|
| `new_in(conversation, sender?, kind, content, idempotency_key, reply_to?)` | 多个 | `Result<Self, MessageError>` | O(1) | 走 `try_send_message` 5 步 |
| `try_transition(event)` | `MessageEvent` | `Result<Self, TransitionError>` | O(1) | 见 `aux-04 §E.1` trait 草案 |
| `validate_content(kind, content)` | 消息类型 + JSONB | `Result<(), MessageError>` | O(n) | 按 kind 校验 schema |
| `edit(content)` | 新 content | `Result<Self, MessageError>` | O(1) | 走 `MessageService::edit_message`,需在 recall_window 内 |
| `recall(actor)` | sender / admin | `Result<Self, MessageError>` | O(1) | 走 `MessageService::recall_message`,需在 recall_window 内 |
| `react(user, emoji)` | 反应者 + emoji | `()` | O(1) | INSERT message_reactions,主键 (message_id, user_id, emoji) |
| `mark_read(user, sequence)` | 接收者 + seq | `()` | O(1) | UPDATE conversation_members.last_read_sequence |

**测试要点 (Test Points)**

- 正常: 发送 text 消息 → state=sent + sequence 严格单调 + WS fanout
- 正常: 5 客户端 mark_read → last_read_sequence 单调上升
- 正常: 系统消息(sender_id=NULL)→ INSERT 走 `idempotency_key` 去重
- 异常: 同 `idempotency_key` 重复发 5 次 → 1 条消息 + 5 次相同 message_id
- 异常: content 超过 `IM_MESSAGE_MAX_SIZE_BYTES` (默认 64KB) → 400 MESSAGE_TOO_LARGE
- 异常: reply_to 指向不存在的 message → MESSAGE_NOT_FOUND
- 异常: sender = 已被 deleted 的 user → FORBIDDEN(状态机拦截)
- 异常: sender ≠ 当前 actor 试图 edit → FORBIDDEN
- 异常: edit 超 recall_window → RECALL_WINDOW_EXPIRED
- 异常: 状态机非法转换(例如 recalled → sent)→ INVALID_STATE_TRANSITION
- 边界: reply_to 指向 recalled 消息 → 仍允许,UI 自行处理显示
- 边界: 1000 并发同会话 send_message → sequence 严格单调,无重复
- 边界: content 边界 64KB-1B / 64KB / 64KB+1B

**示例签名**

```rust
// crates/im-core/src/message/message.rs(待 C-2 实装)
#[derive(Debug, Clone)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub sequence: i64,
    pub sender_id: Option<UserId>,        // None = 系统消息
    pub kind: MessageKind,                // enum: Text | Image | File | Sticker | System | Custom
    pub content: serde_json::Value,
    pub reply_to: Option<MessageId>,
    pub idempotency_key: String,
    pub state: MessageState,              // enum: Sent | Delivered | Read | Recalled | Deleted
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
}

impl Message {
    pub fn try_transition(self, event: MessageEvent) -> Result<Self, TransitionError> { /* ... */ }
    pub fn validate_content(kind: MessageKind, content: &serde_json::Value) -> Result<(), MessageError> { /* aux-13 §4.1 schema */ }
    pub async fn try_send_message(svc: &MessageService, cmd: SendMessageCommand) -> Result<Self, MessageError> { /* 5 步流程 */ }
}
```

---

### CRC-004 FriendRequest

| 维度 | 内容 |
|---|---|
| 所在模块 | `crates/im-core/src/relationship/` |
| 类型 | 实体(Entity, 由 Friendship 聚合管理) |
| 关键字段 | `id` (UUID), `environment_id` (UUID), `sender_id` (UUID), `recipient_id` (UUID), `state` (pending/accepted/rejected/expired), `created_at`, `updated_at` |
| 状态机 | `aux-04 §B.2`(4 状态,`accepted`/`rejected`/`expired` 终态) |
| 核心约束 | `UNIQUE (environment_id, sender_id, recipient_id)`(`migrations/0003` 第 21 行,跨所有 state) + `CHECK (sender_id <> recipient_id)` |

**职责 (Responsibilities)**

- [ ] 持有好友申请身份
- [ ] 校验 `sender_id != recipient_id`(DB CHECK 强制)
- [ ] 维护 `state` 状态机(只允许经 `try_transition` 转换,见 `aux-04 §B.2`)
- [ ] 提供 `accept(recipient) -> Friendship`(同步 INSERT `friendships` row,双向语义只存 1 行)
- [ ] 提供 `reject(recipient)`(终态)
- [ ] 写 `audit_logs` 当 `state` 变更(尤其是 `accepted` / `rejected`)

**协作者 (Collaborators)**

- → `RelationshipService`: send / respond / block
- → `FriendshipRepository`: 持久化
- → `Friendship`(作为关联聚合): accept 时同步创建
- → `User`(作为 sender / recipient): 关联 2 个 user
- ← `WsSession`: 接收 `friend_request` 事件后通知 recipient

**不在职责范围内 (Not responsible for)**

- ❌ 不负责好友关系本身的状态管理(`Friendship` 独立聚合,见 `aux-04 §B.3`)
- ❌ 不负责"双向"关系存储(IM1.0 单边 1 行,双向语义由应用层 join)
- ❌ 不负责 `friendships.blocked`(独立转换,见 `aux-04 §B.3`)
- ❌ 不负责通知推送(由 im-presence / WsSession 负责)

**关键方法 (Key Methods)**

| 方法 | 入参 | 出参 | 复杂度 | 备注 |
|---|---|---|---|---|
| `new(env, sender, recipient)` | 构造参数 | `Result<Self, RelationshipError>` | O(1) | 走 `RelationshipService::send_request`,需先检查 `is_blocked` |
| `try_transition(event)` | `FriendRequestEvent` | `Result<Self, TransitionError>` | O(1) | 见 `aux-04 §E.1` trait 草案 |
| `accept(responder)` | `UserId`(必须 == recipient) | `Result<Friendship, RelationshipError>` | O(1) | UPDATE + INSERT friendships |
| `reject(responder)` | `UserId`(必须 == recipient) | `Result<Self, RelationshipError>` | O(1) | UPDATE state=rejected |
| `is_responder(user_id)` | `UserId` | `bool` | O(1) | user_id == recipient |

**测试要点 (Test Points)**

- 正常: A 发送好友申请给 B → 1 行 friend_requests(state=pending)
- 正常: B accept → 1 行 friend_requests(state=accepted) + 1 行 friendships(A→B, state=accepted)
- 正常: B reject → 1 行 friend_requests(state=rejected)
- 正常: 同对用户同 env 重复发申请 → FRIEND_REQUEST_EXISTS 错误(被 UNIQUE 阻断)
- 异常: A → A 自指 → DB CHECK 拒绝(sender_id <> recipient_id)
- 异常: 非 recipient 试图 respond → FORBIDDEN
- 异常: 重复 respond 已 accepted 申请 → INVALID_STATE_TRANSITION
- 异常: B 已被 A 拉黑 → 申请创建时 `is_blocked(A, B)` 拦截 → USER_BLOCKED 错误
- 边界: env 切换后同用户对能否再申请? 答: 可以(env_id 隔离)
- 并发: 1000 并发同对用户申请 → 1 行(`UNIQUE` 阻断,100 次中 999 次 `FRIEND_REQUEST_EXISTS`)

**示例签名**

```rust
// crates/im-core/src/relationship/friend_request.rs(待 C-7 实装)
#[derive(Debug, Clone)]
pub struct FriendRequest {
    pub id: FriendRequestId,
    pub environment_id: EnvironmentId,
    pub sender_id: UserId,
    pub recipient_id: UserId,
    pub state: FriendRequestState,       // enum: Pending | Accepted | Rejected | Expired
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FriendRequest {
    pub fn try_transition(self, event: FriendRequestEvent) -> Result<Self, TransitionError> { /* ... */ }
    pub async fn accept(svc: &RelationshipService, responder: UserId) -> Result<Friendship, RelationshipError> { /* 校验 responder == recipient */ }
    pub fn is_responder(&self, user_id: UserId) -> bool { self.recipient_id == user_id }
}
```

---

## F. 服务层 CRC 索引(待 V1+ 补充)

> MVP 阶段服务层在 WBS C-1..C-12 中实装,本表预留指针,V1+ 补全。

| 服务 | 模块 | 责任 |
|---|---|---|
| `IdentityService` | `crates/im-core/identity/service.rs` | 鉴权 / 状态变更 / 资料更新(`ImplementationSpec §7.4.1`) |
| `ConversationService` | `crates/im-core/conversation/service.rs` | 创建 / 列表 / 成员管理(`ImplementationSpec §7.4.2`) |
| `MessageService` | `crates/im-core/message/service.rs` | 5 步 send / edit / recall / react / mark_read(`ImplementationSpec §7.4.3`) |
| `RelationshipService` | `crates/im-core/relationship/service.rs` | 好友申请 / 响应 / 拉黑(`ImplementationSpec §7.4.4`) |
| `SettingsService` | `crates/im-core/settings/service.rs` | `environments.settings` 加载 + Valkey 缓存 + NATS 失效监听(`ImplementationSpec §7.4.6`) |
| `TokenService` | `crates/im-core/identity/token.rs` | JWT 签发 / 校验 / 旋转(`ImplementationSpec §7.4.1`) |
| `WsSession` | `crates/im-gateway/ws/session.rs` | WS 帧路由 / 鉴权 / heartbeat / fanout(`ImplementationSpec §7.5`) |

## G. 验收标准 (Acceptance Criteria)

- [ ] 4 张核心类 CRC(User / Conversation / Message / FriendRequest)填实,字段、状态机、协作者、不在职责 100% 覆盖
- [ ] 每个 CRC 至少 3 个职责 / 2 个协作者 / 2 个"不在职责"
- [ ] 每个 CRC 至少 4 个测试要点(正常 / 异常 / 边界 / 并发)
- [ ] 字段引用与 `aux-02 §F.4/§F.6/§F.8/§F.12` 100% 一致(已交叉验证)
- [ ] 状态机引用与 `aux-04 §B` 100% 一致
- [ ] Trait 引用与 `ImplementationSpec §7.4` 100% 一致
- [ ] 服务层 CRC 索引指向 `ImplementationSpec §7.4.1-7.4.6` 已存在段落

## H. 关联文档 (References)

- 类设计: `docs/DetailedDesign.md` §9(im-core 模块设计)
- Rust trait 完整清单: `docs/ImplementationSpec.md` §7.4(im-core)+ §7.5(im-gateway)
- 字段字典: `aux-02-data-dictionary.md` §F.4(users)/ §F.5(device_sessions)/ §F.6(friend_requests)/ §F.7(friendships)/ §F.8-F.11(conversations 相关)/ §F.12(messages)/ §F.13(message_reactions)
- 状态机: `aux-04-state-machine-spec.md` §B.1(users)/ §B.2(friend_requests)/ §B.3(friendships)/ §B.4(messages)/ §B.5(conversations)
- 错误码: `aux-03-error-code-registry.md` §B
- 协议帧: `aux-13-protocol-frame-samples.md` §1(WS 12 类帧)+ §3(REST 26 端点)
- 命名规范: `aux-01-naming-convention.md` §B(Rust 命名)+ §G(业务术语)
- DB schema: `migrations/0002_create_users_device_sessions.sql` / `0003_create_friend_requests_and_friendships.sql` / `0004_create_conversations_sequences_members_dm_pairs.sql` / `0005_create_messages_reactions.sql`
- REST API 端点: `ImplementationSpec §3.1`

## I. 已知缺口 (Known Gaps)

| 编号 | 缺口 | 影响 | 跟进 |
|---|---|---|---|
| GAP-1 | 值对象 CRC(MessageContent / MessageKind / ExternalIdentity / DisplayName 等)未列 | 编码时无值对象职责指引 | V1+ 补充 |
| GAP-2 | 仓储层 CRC(UserRepository / ConversationRepository 等)未列 | repo trait 在 `ImplementationSpec §7.4` 有,但 CRC 形式未出 | V1+ 补 |
| GAP-3 | 服务层 CRC 仅列索引,未实装 | 评审服务层时无对应 CRC 可查 | WBS C-1..C-12 实装后, V1+ 回填 |
| GAP-4 | WsSession / NatsEventPublisher / RpcError 等 im-gateway / 跨服务类未列 | im-gateway 是独立进程,职责卡重要 | V1+ 补 |
| GAP-5 | 测试要点未对应具体单元测试文件路径 | 单元测试位置待 C-1..C-12 实装时定 | WBS C-1..C-12 |
| GAP-6 | `DeviceSession` 独立 CRC 缺失 | 重要聚合(与 Token 旋转紧密相关) | V1+ 补 |
| GAP-7 | `Environment` / `Game` / `Tenant` 顶层管理实体 CRC 缺失 | MVP 阶段管理面未实装,留 V1+ | V1+ |
| GAP-8 | `AuditLog` 实体 CRC 缺失 | 与所有状态机联动,职责分散 | V1+ 补 |
| GAP-9 | `Media`(im-media 占位 crate)CRC 缺失 | 仅 1 个 RPC(PresignMedia),V1+ 实装时再补 | V1+ |

## J. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 填实 IM1.0 4 大核心类 CRC(User / Conversation / Message / FriendRequest);§F 服务层 CRC 索引(指向 ImplementationSpec §7.4);§I 9 项已知缺口;引用 aux-02/03/04/13 + ImplementationSpec §7 + migrations 实际行号 |
