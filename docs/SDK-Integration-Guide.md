# Game SDK 集成指南（MVP / Unity）

版本：v0.1 Draft ｜ 面向：接入本 IM 平台的游戏客户端与服务端工程师
上游依据：`docs/SRS.md`（SDK-FR-*, GAME-FR-*, GAME-ID-*）、`docs/DetailedDesign.md`（协议细节）

本指南只覆盖 **MVP 范围**：Unity 客户端、Guest/自定义 JWT 身份、Party 场景之外的基础文本消息收发（语音 Party/Guild 等见 `docs/LiveKit-Voice-Subsystem.md`，非本指南 MVP 范围）。

---

## 1. 集成前置条件

- 一个 Tenant / Game / Environment（由平台运营方或自助控制台创建，返回 `environment_id`）。
- 一个 **Server Secret**（按 Environment 签发，仅保存在你的游戏服务器，**绝不下发到客户端**，呼应 SRS `GAME-ID-003` 红线）。
- 游戏客户端可访问 `wss://gateway.{tenant}.example.com` 与 `https://gateway.{tenant}.example.com`（占位域名，实际以平台运营方提供为准）。

## 2. 身份接入两种路径

### 2.1 Guest（无需游戏账号体系，最快接入）

```
客户端 → POST /v1/auth/guest  { environment_id }
       ← { access_token, refresh_token, user_id }
```

客户端直接调用，无需游戏服务器参与。**适用场景**：快速原型、无强身份要求的休闲游戏。**限制**：Guest 身份未与游戏账号绑定，卸载重装后是新身份（除非应用侧自行持久化 refresh_token）。

### 2.2 游戏账号绑定（推荐用于正式接入）

```
游戏客户端登录游戏账号（Steam/自有账号体系等，与本IM平台无关）
        │
        ▼
游戏客户端请求游戏服务器下发IM访问凭证
        │
        ▼
游戏服务器 → POST /v1/auth/token/exchange
             { environment_id, external_provider, external_uid, display_name? }
             Header: Authorization: HMAC-SHA256(server_secret, body)
        ← { access_token, refresh_token, user_id }
        │
        ▼
游戏服务器将 access_token 下发给客户端（通过你自己已有的安全信道，如已加密的游戏登录响应）
        │
        ▼
客户端使用该 access_token 完成 SDK.Authenticate()
```

**关键红线**（呼应 `GAME-ID-003`/`SEC-NFR-005`）：`server_secret` 永远只存在于游戏服务器；客户端拿到的是**已经由服务器代为签发**的 `access_token`，客户端自身无法伪造或提升权限。

## 3. Unity SDK 五步接入代码骨架

```csharp
using ImPlatform;

// 1. Initialize
var config = new ImConfig {
    GatewayUrl = "wss://gateway.your-tenant.example.com",
    EnvironmentId = "env_xxx",
};
ImClient.Initialize(config);

// 2. Authenticate（access_token 来自 §2.1 或 §2.2）
await ImClient.Instance.AuthenticateAsync(accessToken);

// 3. Connect
await ImClient.Instance.ConnectAsync();
ImClient.Instance.OnMessageReceived += (msg) => {
    Debug.Log($"[{msg.ConversationId}] {msg.SenderId}: {msg.Content}");
};
ImClient.Instance.OnConnectionStateChanged += (state) => {
    Debug.Log($"connection state = {state}");
};

// 4. Join（加入/订阅一个会话；会话需已存在，或由服务端/Game Extension自动创建）
await ImClient.Instance.JoinConversationAsync(conversationId);

// 5. Send
await ImClient.Instance.SendMessageAsync(conversationId, new TextContent { Text = "hello" });
```

SDK 内部自动处理：断线重连（指数退避，初始1s，上限30s）、心跳（30s间隔，呼应 `DetailedDesign.md §10`）、`req_id`/`idempotency_key` 生成与去重、本地会话游标（`last_known_sequence`）维护。

## 4. 离线消息与增量同步（客户端无需手写，但需理解行为）

- SDK 在 `ConnectAsync()` 成功后，会对每个本地已知的 `conversation_id` 自动调用 `GET /v1/conversations/{id}/messages?after_sequence={local_cursor}` 拉取断线期间的新消息，再切换到 WebSocket 实时推送。
- 应用层**不需要**自己实现离线队列；若需要在应用重启后仍保留会话游标，需自行持久化 SDK 暴露的 `ConversationCursor` 到本地存储（SDK 默认仅存内存，跨进程持久化是应用层职责，避免 SDK 强加存储方案）。

## 5. 常见接入错误与排查

| 现象 | 可能原因 | 排查方向 |
|---|---|---|
| `AuthenticateAsync` 返回 `UNAUTHORIZED` | access_token 过期/签名错误 | 检查 Token TTL（默认候选值15分钟），确认游戏服务器签发流程正确 |
| 连接后收不到消息 | 未调用 `JoinConversationAsync`，或该用户不是会话成员 | 检查 `conversation_members` 是否包含该用户；会话是否由服务端正确创建 |
| `SendMessageAsync` 一直超时 | 网络问题，或触发 `RATE_LIMITED` | 查看 SDK 日志中的 `ack.error.code`，参考 `DetailedDesign.md §7` 错误码表 |
| 重复收到同一条消息 | 客户端未正确处理 `idempotency_key` 去重，或应用层重复调用 SendMessage | 确认每次用户操作只生成一个 `idempotency_key`，不要在重试时重新生成 |

## 6. 与游戏社交概念（Guild/Party）的关系（V1能力预告，MVP暂不提供官方封装）

`docs/SRS.md` 第21章定义了 Guild/Party 等概念通过 `Conversation.metadata` 映射，但 **MVP 阶段 Unity SDK 不提供 `Voice.JoinParty()` 这类语音/社交封装**（语音在 `docs/LiveKit-Voice-Subsystem.md` 中单独设计，V1交付）。MVP 阶段若需要"Party 文字频道"，由你的游戏服务器在 Party 建立时调用 `POST /v1/conversations`（`kind=group`, `metadata.game.type=party`）自行创建并把成员加入，SDK 侧仅按普通 Conversation 处理。

## 7. Console / 其他引擎现状（如实告知，不承诺）

- **Unreal/Godot**：MVP 不提供官方 SDK，参见 `docs/LiveKit-Voice-Subsystem.md` 第25/26章的同一结论——Unreal 需自建 Rust Core + C ABI Adapter，非MVP交付物；纯文字IM场景可直接走本平台的 REST/WebSocket 协议自行封装（协议见 `docs/DetailedDesign.md` 第3/5章），不强制等待官方SDK。
- **Console（PlayStation/Xbox/Switch）**：状态为 Unknown/Requires Vendor Approval，与语音章节结论一致，不在MVP承诺范围。
