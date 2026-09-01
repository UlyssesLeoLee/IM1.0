---
doc_id: aux-13
title_ja: プロトコルフレームサンプル集 (IM1.0)
title_zh: 协议帧样例集 (IM1.0)
phase: 04-detailed-design-aux
owners: Tech Lead
status: Filled (v1.0.0)
version: 1.1.1
related_activities: 46 API 详细, 28 API 仕様, 29 IF 詳細
patch_note: |
  2026-09-01 B-4 协议冻结补丁 [PROTOCOL-FROZEN-PATCH]:
  补 ImplementationSpec §16 P2-3 标记的缺失样例 — RespondFriendRequest
  gRPC request + POST /v1/friends/requests/{id}/respond REST body。
  不新增协议元素(端点 / RPC 早于 2026-08-26 [PROTOCOL-FROZEN] 冻结),
  仅补 [PROTOCOL-FROZEN] 状态下遗漏的样例。aux-13 §7 流程豁免理由:
  "补缺失样例,非新元素"。
---

# aux-13. プロトコルフレームサンプル集 (IM1.0) / 协议帧样例集 (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: Tech Lead
> 协议源: `docs/DetailedDesign.md` §2 (gRPC) / §3 (WebSocket) / §5 (REST)
> 命名: `conversation` / `message` / `user`(详见 `aux-01` §G)

## 1. 目的 (Purpose)

为 IM1.0 各协议(WS / gRPC / REST)提供完整可复制的帧样例,联调 / 排障 / 自动化测试 / 客户支持 直接对照引用,所有示例均与 `DetailedDesign.md` §2/§3/§5 严格一致。

## 2. 适用范围 (Scope)

- im-gateway 对外 WebSocket 协议(§1)
- im-gateway ⇄ im-core 内部 gRPC 协议(§2)
- im-gateway 对外 REST API(§3)
- 通用错误响应格式(§4)
- 调试用 curl / wscat 命令(§5)
- 协议版本约定(§6)

## 3. 责任方 (Owners)

Tech Lead。任何协议变更必须同时更新本表与 `DetailedDesign.md` 相应章节。

## 4. 前置依赖 (Prerequisites / Inputs)

- `docs/DetailedDesign.md` §2-§5(本表内容源)
- `docs/aux-03-error-code-registry.md`(错误码枚举)

## 5. 输出 / 模板正文 (Body)

## 1. WebSocket 帧 (im-gateway ⇄ Client)

帧格式:JSON over WebSocket Text Frame(MVP 选 JSON,二进制 Protobuf 为 ADR-014 Candidate,见 `DetailedDesign.md §3` + `BasicDesign.md §16`)。

所有客户端发起的**写操作**必须携带 `req_id`(客户端生成 UUID v4),服务端 `ack` 回带同一 `req_id`,客户端据此匹配请求-响应。

### 1.1 客户端 → 服务端 帧

#### 1.1.1 `auth`(WebSocket 鉴权)

```json
{
  "type": "auth",
  "req_id": "11111111-1111-4111-8111-111111111111",
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
}
```

#### 1.1.2 `send_message`(发消息)

```json
{
  "type": "send_message",
  "req_id": "22222222-2222-4222-8222-222222222222",
  "conversation_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "idempotency_key": "33333333-3333-4333-8333-333333333333",
  "kind": "text",
  "content": { "text": "你好" },
  "reply_to": null
}
```

#### 1.1.3 `edit_message`(编辑)

```json
{
  "type": "edit_message",
  "req_id": "44444444-4444-4444-8444-444444444444",
  "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
  "content": { "text": "你好(已编辑)" }
}
```

> **约束**:只能在消息创建后 `IM_MESSAGE_RECALL_WINDOW_SECONDS` 内编辑(默认同撤回窗口, Candidate),且仅 sender 可编辑;超过窗口返回 `RECALL_WINDOW_EXPIRED` 错误(用 `edit_window_expired` 子语义,本 MVP 与 recall 共享错误码)。

#### 1.1.4 `recall_message`(撤回)

```json
{
  "type": "recall_message",
  "req_id": "55555555-5555-4555-8555-555555555555",
  "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7"
}
```

#### 1.1.5 `react`(添加 reaction)

```json
{
  "type": "react",
  "req_id": "66666666-6666-4666-8666-666666666666",
  "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
  "emoji": "👍"
}
```

#### 1.1.6 `mark_read`(上报已读)

```json
{
  "type": "mark_read",
  "req_id": "77777777-7777-4777-8777-777777777777",
  "conversation_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "sequence": 42
}
```

#### 1.1.7 `typing`(输入中指示)

```json
{
  "type": "typing",
  "req_id": "88888888-8888-4888-8888-888888888888",
  "conversation_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7"
}
```

#### 1.1.8 `ping`(心跳)

```json
{
  "type": "ping",
  "ts": 1692528000000
}
```

> 客户端每 30s 发一次,服务端 60s 未收到任何帧视为死连接主动断开(`IM_WS_HEARTBEAT_TIMEOUT_SECONDS=60`,见 `DetailedDesign.md §10`)。

### 1.2 服务端 → 客户端 帧

#### 1.2.1 `connected`(WS 握手成功)

```json
{
  "type": "connected",
  "session_id": "9b8e6679-7425-40de-944b-e07fc1f90ae7"
}
```

> `session_id` = `device_sessions.id`,客户端可用于日志关联。

#### 1.2.2 `ack`(请求成功,带 req_id)

```json
{
  "type": "ack",
  "req_id": "22222222-2222-4222-8222-222222222222",
  "ok": true,
  "data": {
    "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
    "sequence": 42
  }
}
```

#### 1.2.3 `ack` 失败(幂等冲突的特殊语义)

```json
{
  "type": "ack",
  "req_id": "22222222-2222-4222-8222-222222222222",
  "ok": true,
  "data": {
    "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
    "sequence": 42,
    "idempotent_replay": true
  }
}
```

> **关键**:`IDEMPOTENCY_CONFLICT` 在 WS 协议中**以成功形式返回**(`ok=true` + `idempotent_replay=true`),与 REST 路径的 HTTP 200 同语义。客户端不需要分支处理。

#### 1.2.4 `ack` 失败(真错误)

```json
{
  "type": "ack",
  "req_id": "22222222-2222-4222-8222-222222222222",
  "ok": false,
  "error": {
    "code": "RATE_LIMITED",
    "message": "auth.rate_limit.send_message",
    "trace_id": "tr_01HXY..."
  }
}
```

> `error.code` 取值见 `aux-03-error-code-registry.md` §B。

#### 1.2.5 `message_new`(广播新消息)

```json
{
  "type": "message_new",
  "message": {
    "id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
    "conversation_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
    "sequence": 42,
    "sender_id": "1a2e6679-7425-40de-944b-e07fc1f90ae7",
    "kind": "text",
    "content": { "text": "你好" },
    "reply_to": null,
    "state": "sent",
    "created_at": "2026-08-23T00:00:00Z",
    "edited_at": null,
    "reactions": []
  }
}
```

#### 1.2.6 `message_edited`

```json
{
  "type": "message_edited",
  "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
  "content": { "text": "你好(已编辑)" },
  "edited_at": "2026-08-23T00:01:00Z"
}
```

#### 1.2.7 `message_recalled`

```json
{
  "type": "message_recalled",
  "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
  "conversation_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7"
}
```

#### 1.2.8 `reaction_added`

```json
{
  "type": "reaction_added",
  "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
  "user_id": "2b3e6679-7425-40de-944b-e07fc1f90ae7",
  "emoji": "👍"
}
```

#### 1.2.9 `presence_update`

```json
{
  "type": "presence_update",
  "user_id": "2b3e6679-7425-40de-944b-e07fc1f90ae7",
  "status": "online"
}
```

> `status` ∈ {`online`, `offline`, `away`, `busy`, `invisible`}。

#### 1.2.10 `typing`

```json
{
  "type": "typing",
  "conversation_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "user_id": "2b3e6679-7425-40de-944b-e07fc1f90ae7"
}
```

#### 1.2.11 `pong`

```json
{
  "type": "pong",
  "ts": 1692528000000
}
```

#### 1.2.12 `force_disconnect`

```json
{
  "type": "force_disconnect",
  "reason": "token_revoked"
}
```

> `reason` ∈ {`token_revoked`, `account_banned`, `account_deleted`, `admin_kick`}。客户端收到后应停止重连并跳登录。

### 1.3 WebSocket 协议规则总结

| 规则 | 描述 |
|---|---|
| 帧编码 | JSON over WS Text Frame(UTF-8) |
| 请求-响应配对 | 客户端写操作必带 `req_id`,服务端 `ack` 回带 |
| 心跳 | 客户端 30s `ping`,服务端 60s 无帧超时 |
| 重连 | 客户端负责,使用 `after_sequence` 增量拉取(不在 WS 层做服务端补发) |
| 顺序保证 | 同一会话内消息 `sequence` 单调递增;WS 帧顺序按服务端发送顺序 |
| 错误语义 | `IDEMPOTENCY_CONFLICT` 走成功语义(详见 §1.2.3);其他错误 `ok=false` |

## 2. gRPC 消息 (im-gateway ⇄ im-core, package `im.core.v1`)

> 完整 proto 文件: `crates/im-proto/proto/core.proto`(由 `tonic-build` 生成)。

### 2.1 `SendMessage`

```protobuf
service CoreService {
  rpc SendMessage(SendMessageRequest) returns (Message);
}

message SendMessageRequest {
  string conversation_id = 1;
  string sender_id = 2;            // 来自 im-gateway 校验后的 claims
  string idempotency_key = 3;
  MessageKind kind = 4;
  bytes content_json = 5;          // 序列化的 JSON content(见 aux-13 §4.1)
  optional string reply_to = 6;
}

message Message {
  string id = 1;
  string conversation_id = 2;
  int64 sequence = 3;
  optional string sender_id = 4;
  MessageKind kind = 5;
  bytes content_json = 6;
  optional string reply_to = 7;
  string state = 8;                 // sent/delivered/read/recalled/deleted
  google.protobuf.Timestamp created_at = 9;
  optional google.protobuf.Timestamp edited_at = 10;
}

enum MessageKind {
  MESSAGE_KIND_UNSPECIFIED = 0;
  TEXT = 1;
  IMAGE = 2;
  FILE = 3;
  STICKER = 4;
  SYSTEM = 5;
  CUSTOM = 6;
}
```

### 2.2 `ListMessages`(增量拉取)

```protobuf
rpc ListMessages(ListMessagesRequest) returns (ListMessagesResponse);

message ListMessagesRequest {
  string conversation_id = 1;
  int64 after_sequence = 2;        // 0 = 从头
  int32 limit = 3;                 // default 50, max 200
}

message ListMessagesResponse {
  repeated Message messages = 1;
  bool has_more = 2;
  int64 latest_sequence = 3;       // 客户端更新本地游标
}
```

### 2.3 `ValidateAccessToken`(WS 鉴权用)

```protobuf
rpc ValidateAccessToken(ValidateAccessTokenRequest) returns (ValidateAccessTokenResponse);

message ValidateAccessTokenRequest {
  string access_token = 1;
}

message ValidateAccessTokenResponse {
  bool valid = 1;
  string user_id = 2;
  string environment_id = 3;
  string tenant_id = 4;
  int64 expires_at_unix = 5;
}
```

### 2.4 `MarkRead`

```protobuf
rpc MarkRead(MarkReadRequest) returns (google.protobuf.Empty);

message MarkReadRequest {
  string conversation_id = 1;
  string user_id = 2;
  int64 sequence = 3;
}
```

### 2.5 `RespondFriendRequest`(好友申请接受/拒绝) — B-4 补 (2026-09-01)

> **补丁来源**:ImplementationSpec §16 P2-3 已知缺口 — `respond_friend_request`
> gRPC/REST 端点的 body `{accept: bool}` 在 v1.1.0 冻结前未补样例。
> 本节为 `[PROTOCOL-FROZEN-PATCH]`(见 §10 change log),端点本身早在
> `crates/im-proto/proto/core.proto` (commit `c6cdc76` 2026-08-24
> 填实) 与 `ImplementationSpec §3.1.4` (POST 路径) 同步冻结,
> 本节仅补"调用样例"。

完整 proto 定义(`crates/im-proto/proto/core.proto` §Relationship 块):

```protobuf
message RespondFriendRequestRequest {
  string request_id = 1;     // friend_requests.id
  string responder_id = 2;   // 来自 im-gateway 校验后的 claims
                            // 必须 == friend_requests.recipient_id
                            // 否则 PERMISSION_DENIED
  bool accept = 3;          // true = 接受, false = 拒绝
                            // 其他状态(accepted/rejected) 重发 → FAILED_PRECONDITION
                            // (INVALID_STATE_TRANSITION)
}

message BlockUserRequest {
  string user_id = 1;
  string target_id = 2;
}

message ListFriendsRequest {
  string user_id = 1;
  string cursor = 2;          // 首次传空,后续传响应的 next_cursor
  int32 limit = 3;            // default 50, max 200
}

message Friend {
  string user_id = 1;
  string display_name = 2;
  string state = 3;           // accepted / blocked(被当前 user 拉黑的不返回)
  google.protobuf.Timestamp since = 4;
}

message ListFriendsResponse {
  repeated Friend friends = 1;
  string next_cursor = 2;
}

service CoreService {
  // ---- Relationship (3) ----
  rpc SendFriendRequest(SendFriendRequestRequest) returns (google.protobuf.Empty);
  rpc RespondFriendRequest(RespondFriendRequestRequest) returns (google.protobuf.Empty);
  rpc BlockUser(BlockUserRequest) returns (google.protobuf.Empty);
  rpc ListFriends(ListFriendsRequest) returns (ListFriendsResponse);
}
```

**gRPC 调用示例**(im-gateway ⇄ im-core, plaintext 调试用):

```bash
grpcurl -plaintext -d '{
  "request_id":   "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "responder_id": "2b3e6679-7425-40de-944b-e07fc1f90ae7",
  "accept":       true
}' \
  localhost:9001 im.core.v1.CoreService/RespondFriendRequest
```

**响应**: `google.protobuf.Empty` (空) — 成功。

**错误码映射**(对应 `aux-03` §B):
- `NOT_FOUND` (`FRIEND_REQUEST_NOT_FOUND`):`request_id` 不存在
- `PERMISSION_DENIED` (`FORBIDDEN`):`responder_id != recipient_id`
- `FAILED_PRECONDITION` (`INVALID_STATE_TRANSITION`):`state != 'pending'`

### 2.6 错误 → gRPC Code 映射

按 `aux-03 §B` 的 HTTP 列同构映射到 `tonic::Code`,`im-gateway` 边界转换:

| IM1.0 错误码 | HTTP | tonic::Code |
|---|---|---|
| `UNAUTHORIZED` | 401 | `UNAUTHENTICATED` |
| `FORBIDDEN` / `ACCOUNT_BANNED` / `ACCOUNT_SUSPENDED` / `USER_BLOCKED` | 403 | `PERMISSION_DENIED` |
| `NOT_FOUND` / `CONVERSATION_NOT_FOUND` / `MESSAGE_NOT_FOUND` | 404 | `NOT_FOUND` |
| `INVALID_STATE_TRANSITION` / `RECALL_WINDOW_EXPIRED` / `ACCOUNT_MERGE_CONFLICT` / `FRIEND_REQUEST_EXISTS` | 409 | `FAILED_PRECONDITION` |
| `VALIDATION_ERROR` / `MESSAGE_TOO_LARGE` / `INVALID_IDEMPOTENCY_KEY` | 400 | `INVALID_ARGUMENT` |
| `RATE_LIMITED` | 429 | `RESOURCE_EXHAUSTED` |
| `INTERNAL_ERROR` | 500 | `INTERNAL` |
| `SERVICE_UNAVAILABLE` | 503 | `UNAVAILABLE` |
| `IDEMPOTENCY_CONFLICT` | 200 | `OK`(特殊语义) |

## 3. REST API (im-gateway 对外)

完整清单见 `DetailedDesign.md §5`。本节给出**关键端点**的请求/响应/错误样例。

### 3.1 `POST /v1/auth/token/exchange`(游戏服务器 Token 兑换)

**请求**:

```http
POST /v1/auth/token/exchange HTTP/1.1
Host: api.{tenant}.example.com
Content-Type: application/json
X-IM-Server-Signature: hex(HMAC-SHA256(server_secret, body))
X-IM-Timestamp: 1692528000
X-IM-Nonce: random-32-bytes

{
  "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "external_provider": "steam",
  "external_uid": "76561198000000000",
  "display_name": "Player1"
}
```

**响应 (200)**:

```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "rt_9b8e6679-7425-40de-944b-e07fc1f90ae7",
  "user_id": "1a2e6679-7425-40de-944b-e07fc1f90ae7",
  "expires_in": 900
}
```

**错误 (401)**:

```json
{
  "code": "UNAUTHORIZED",
  "message": "auth.server_signature.invalid",
  "trace_id": "tr_01HXY...",
  "ts": 1692528000000
}
```

### 3.2 `POST /v1/auth/guest`(Guest 注册)

**请求**:

```http
POST /v1/auth/guest HTTP/1.1
Host: api.{tenant}.example.com
Content-Type: application/json

{ "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7" }
```

**响应 (200)**:同 §3.1。

### 3.3 `POST /v1/conversations`(创建会话)

**请求**:

```http
POST /v1/conversations HTTP/1.1
Host: api.{tenant}.example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
Content-Type: application/json

{
  "kind": "dm",
  "member_user_ids": ["2b3e6679-7425-40de-944b-e07fc1f90ae7"]
}
```

**响应 (201)**:

```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "environment_id": "...",
  "kind": "dm",
  "metadata": {},
  "created_at": "2026-08-23T00:00:00Z"
}
```

**错误 (409)**:

```json
{
  "code": "ACCOUNT_MERGE_CONFLICT",
  "message": "conversation.dm.peer_blocked",
  "trace_id": "tr_01HXY...",
  "ts": 1692528000000
}
```

### 3.4 `GET /v1/conversations/{id}/messages`(增量拉取)

**请求**:

```http
GET /v1/conversations/7c9e6679-7425-40de-944b-e07fc1f90ae7/messages?after_sequence=42&limit=20 HTTP/1.1
Host: api.{tenant}.example.com
Authorization: Bearer eyJ...
```

**响应 (200)**:

```json
{
  "messages": [
    {
      "id": "8a7e6679-...",
      "conversation_id": "7c9e6679-...",
      "sequence": 43,
      "sender_id": "2b3e6679-...",
      "kind": "text",
      "content": { "text": "你好" },
      "reply_to": null,
      "state": "sent",
      "created_at": "2026-08-23T00:00:00Z",
      "edited_at": null,
      "reactions": []
    }
  ],
  "next_cursor": "44",
  "has_more": false
}
```

### 3.5 `POST /v1/conversations/{id}/messages`(REST 发消息,SDK 也可走 REST)

**请求**:

```http
POST /v1/conversations/7c9e6679-7425-40de-944b-e07fc1f90ae7/messages HTTP/1.1
Host: api.{tenant}.example.com
Authorization: Bearer eyJ...
Content-Type: application/json
X-IM-Idempotency-Key: 33333333-3333-4333-8333-333333333333

{
  "kind": "text",
  "content": { "text": "你好" },
  "reply_to": null
}
```

**响应 (200)**:同 `message_new` 帧结构(§1.2.5)。

**幂等冲突响应 (200, 客户端视作成功)**:

```json
{
  "id": "8a7e6679-...",
  "sequence": 42,
  "idempotent_replay": true,
  ...
}
```

### 3.6 `POST /v1/media/presign`(媒体预签名上传)

**请求**:

```http
POST /v1/media/presign HTTP/1.1
Authorization: Bearer eyJ...
Content-Type: application/json

{
  "content_type": "image/png",
  "size_hint": 102400
}
```

**响应 (200)**:

```json
{
  "upload_url": "https://minio.example.com/im-media/...?X-Amz-Signature=...",
  "media_id": "5c6e6679-7425-40de-944b-e07fc1f90ae7",
  "expires_at": "2026-08-23T01:00:00Z"
}
```

### 3.7 `POST /v1/friends/requests/{id}/respond`(好友申请接受/拒绝) — B-4 补 (2026-09-01)

> **补丁来源**:ImplementationSpec §16 P2-3 已知缺口 — `respond_friend_request`
> REST 端点的 body `{accept: bool}` 在 v1.1.0 冻结前未补样例。
> 端点路径已在 `ImplementationSpec §3.1.4` 表格内冻结,
> 本节为 `[PROTOCOL-FROZEN-PATCH]`(见 §10 change log),仅补 body/响应/错误样例。

**完整 curl 示例(接受)**:

```http
POST /v1/friends/requests/7c9e6679-7425-40de-944b-e07fc1f90ae7/respond HTTP/1.1
Host: api.{tenant}.example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
Content-Type: application/json

{
  "accept": true
}
```

**完整 curl 示例(拒绝)**:

```http
POST /v1/friends/requests/7c9e6679-7425-40de-944b-e07fc1f90ae7/respond HTTP/1.1
Host: api.{tenant}.example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
Content-Type: application/json

{
  "accept": false
}
```

**响应 (204 No Content,接受/拒绝 均无 body)**:

```http
HTTP/1.1 204 No Content
```

> 接受时:服务端在事务内同时 UPDATE `friend_requests.state='accepted'` +
> INSERT `friendships` (env, sender_id, friend_id) + INSERT (env, recipient_id, friend_id)
> 两条对偶记录,见 `migrations/0003_create_friend_requests_and_friendships.sql`。
> 拒绝时:仅 UPDATE `friend_requests.state='rejected'`,**不**插入 friendships。

**错误响应**(对应 `aux-03` §B):

**404 FRIEND_REQUEST_NOT_FOUND**(request_id 不存在):

```json
{
  "code": "FRIEND_REQUEST_NOT_FOUND",
  "message": "friend.respond.not_found",
  "trace_id": "tr_01HXY...",
  "ts": 1692528000000
}
```

**403 FORBIDDEN**(调用者不是 recipient):

```json
{
  "code": "FORBIDDEN",
  "message": "friend.respond.not_recipient",
  "trace_id": "tr_01HXY...",
  "ts": 1692528000000,
  "details": [
    { "field": "responder_id", "reason": "must_equal_recipient_id" }
  ]
}
```

**409 INVALID_STATE_TRANSITION**(request.state != 'pending'):

```json
{
  "code": "INVALID_STATE_TRANSITION",
  "message": "friend.respond.already_decided",
  "trace_id": "tr_01HXY...",
  "ts": 1692528000000,
  "details": [
    { "field": "state", "reason": "expected_pending_actual_accepted" }
  ]
}
```

**速率限制**:`RATE_LIMITED` (HTTP 429) — 同一 responder_id 1 分钟内最多 30 次 respond(由 D-4 Valkey 令牌桶强制)。

**幂等性**:重复 respond 已 accepted/rejected 的 request 返回 409 `INVALID_STATE_TRANSITION`(**不**走 IDEMPOTENCY_CONFLICT 成功语义 — 与 send_message 不同,因为社交关系是不可重放副作用)。

### 3.8 通用错误响应格式

所有 REST 错误响应(4xx / 5xx)统一格式:

```json
{
  "code": "VALIDATION_ERROR",
  "message": "request.validation.field_required",
  "trace_id": "tr_01HXY...",
  "ts": 1692528000000,
  "details": [
    { "field": "content.text", "reason": "max_length" }
  ]
}
```

- `code` 来自 `aux-03` 错误码注册表
- `message` 是 i18n key,**非最终用户文案**(前端按当前 locale 翻译)
- `trace_id` 用于服务端日志查询(参见 `aux-09` 日志 cookbook)
- `details` 仅 `VALIDATION_ERROR` 出现,定位具体字段错误

## 4. JSON Schema(消息 content 等)

### 4.1 `Message.content`(按 `kind`)

| kind | content JSON 必填字段 | 可选字段 |
|---|---|---|
| `text` | `text: string(1..=4000)` | — |
| `image` | `media_id: uuid` | `width: int`, `height: int`, `thumbnail_media_id: uuid` |
| `file` | `media_id: uuid`, `file_name: string(1..=255)`, `size_bytes: int(>0)` | `mime_type: string` |
| `sticker` | `sticker_id: string(1..=64)` | — |
| `system` | `event: string(join/leave/kicked/renamed/...)` | `actor_user_id: uuid`, `target_user_id: uuid` |
| `custom` | `schema: string(1..=64)`, `data: object` | (Extension 定义) |

### 4.2 `Conversation.metadata` 命名空间约定

按 SRS `IM-CONV-002`,任何命名空间在 `metadata` 中以 `<namespace>.<key>` 形式,Core 不解释具体语义:

```json
{
  "game.type": "guild|party|match|...",
  "game.game_id": "uuid",
  "game.external_group_id": "string",
  "ai.observer_enabled": true,
  "work.external_id": "string"
}
```

> **红线**:`messages` / `conversations` / `users` 等 Core 表**严禁**出现 `guild_id` / `match_id` 等专有字段,只允许此 JSONB metadata 扩展点。

## 5. 调试用命令

### 5.1 登录(Guest)

```bash
curl -X POST https://api.{tenant}.example.com/v1/auth/guest \
  -H "Content-Type: application/json" \
  -d '{"environment_id":"7c9e6679-7425-40de-944b-e07fc1f90ae7"}'
```

### 5.2 REST 发消息

```bash
TOKEN=$(curl -sX POST https://api.{tenant}.example.com/v1/auth/guest \
  -H "Content-Type: application/json" \
  -d '{"environment_id":"..."}' | jq -r '.access_token')

curl -X POST https://api.{tenant}.example.com/v1/conversations/7c9e6679-.../messages \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -H "X-IM-Idempotency-Key: $(uuidgen)" \
  -d '{"kind":"text","content":{"text":"hello"}}'
```

### 5.3 WebSocket 收发(wscat)

```bash
# 安装:wscat (npm i -g wscat)
wscat -c wss://gateway.{tenant}.example.com/ws \
  -H "Authorization: Bearer $TOKEN"

# 连接后第一帧(可选,若用 query token)
> { "type": "auth", "req_id": "11111111-...", "access_token": "..." }
< { "type": "connected", "session_id": "..." }

# 发消息
> { "type": "send_message", "req_id": "22222222-...", "conversation_id": "7c9e6679-...", "idempotency_key": "33333333-...", "kind": "text", "content": {"text": "hi"} }
< { "type": "ack", "req_id": "22222222-...", "ok": true, "data": {"message_id": "8a7e6679-...", "sequence": 42} }
```

### 5.4 gRPC 调试(grpcurl)

```bash
# 列出服务
grpcurl -plaintext localhost:9001 list

# 校验 token
grpcurl -plaintext -d '{"access_token":"eyJ..."}' \
  localhost:9001 im.core.v1.CoreService/ValidateAccessToken
```

## 6. 协议版本

| 协议 | 版本 | 路径 / 标识 | 升级规则 |
|---|---|---|---|
| WebSocket | v1(候选,见 ADR-014) | 帧 `type` 字段可加 `v1.` 前缀(如 `v1.send_message`);MVP 直接 `send_message` | 不兼容变更升 v2,旧版保留 6 个月 |
| REST | v1 | URL `/v1/` | 不兼容变更升 v2,旧版保留 6 个月 |
| gRPC | v1 | package `im.core.v1` | 不兼容变更升 v2,旧版保留 6 个月 |

> **MVP 当前**:WS 帧 `type` 不带 `v1.` 前缀(简化);协议冻结后任何破坏性变更必须升 v2 并保留 6 个月。
>
> **冻结状态 [PROTOCOL-FROZEN]**(2026-08-26 JST 生效):WS 12 个帧(双向,详见 §1) + gRPC 22 个 RPC(im.core.v1) + REST 26 个端点(im-gateway 对外) 三套协议已冻结。后续任何破坏性变更走 §7 流程(7 天公告 + RFC + Tech Lead + PM review + 6 个月兼容期)。配套 commit 标题含 `[PROTOCOL-FROZEN]` 标签,变更时解除冻结。

## 7. 协议冻结与变更流程

1. **冻结**:MVP 第一个 PR 合并时 WS / gRPC / REST 三套协议冻结,在 PR 中标 `[PROTOCOL-FROZEN]`
2. **变更**:任何破坏性变更必须:
   - 在 `#im-protocol` Slack 频道提前 7 天公告
   - 提交 RFC 描述新旧差异 + 客户端迁移路径
   - Tech Lead + PM 共同 review
   - 实现时同时支持新旧两版(6 个月兼容期)
3. **日志**:所有协议变更写入 `docs/aux-13/CHANGELOG.md`(本表 §8 之外)

## 8. 验收标准 (Acceptance Criteria)

- [ ] 每个协议端点有完整帧样例(本文档已覆盖 12 个 WS 帧 + 4 个 gRPC + 7 个 REST + 通用错误)
- [ ] 样例与代码实现严格一致(由 integration test 验证)
- [ ] 协议变更时同步更新本表 + `DetailedDesign.md`
- [ ] 调试 curl / wscat / grpcurl 命令可直接复用
- [ ] 错误码 100% 与 `aux-03` 一致
- [ ] 命名 100% 与 `aux-01` 一致(`conversation` 而非 `room` / `chat`)

## 9. 关联文档 (References)

- 关联工程活动: 46 API 详细, 28 API 仕様, 29 IF 詳細
- 上游 Workflow: `docs/Workflow.md` Phase 4
- 上游: `docs/DetailedDesign.md` §2-§5(本表内容源)
- 关联: `aux-01-naming-convention.md` §G 业务术语, `aux-02-data-dictionary.md` §F.12 messages 表
- 关联: `aux-03-error-code-registry.md` §B 错误码 → HTTP/gRPC 映射
- 关联: `aux-09-log-query-cookbook.md` trace_id 查询

## 10. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-08-23 | Mavis 辅助 | 填实 IM1.0:§1 WS 12 个帧(双向);§2 gRPC 4 个核心 RPC + proto 示例;§3 REST 7 个端点 + 错误通用格式;§4 JSON Schema 6 种 kind + Conversation metadata 命名空间;§5 调试命令 wscat/grpcurl/curl;§6 协议版本与冻结流程;全表命名从 `room_id`/`chat_rooms` 改为 `conversation_id`/`conversations` 对齐 aux-01 |
| 1.1.1 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | **[PROTOCOL-FROZEN-PATCH]** B-4 补丁(aux-13 §7 流程豁免,理由:补缺失样例非新元素):新增 §2.5 gRPC `RespondFriendRequest` 样例 + proto 块(原错误映射表 §2.5 → §2.6);新增 §3.7 REST `POST /v1/friends/requests/{id}/respond` 接受/拒绝 curl + 204/404/403/409 错误样例(原通用错误格式 §3.7 → §3.8);修复 ImplementationSpec §16 P2-3 已知缺口;不新增协议元素,端点与 RPC 早在 2026-08-26 [PROTOCOL-FROZEN] (commit 12c7662) 冻结 |
