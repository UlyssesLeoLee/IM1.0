# API 快速接入（Integrator Quickstart）

面向**第一次接入本 IM 平台的工程师**。目标：读完这一页能发出第一个请求、开一条
WebSocket、知道每个端点返回什么、知道**哪些东西还没实现**（免得踩空）。

- 机器可读契约：[`docs/api/openapi.json`](openapi.json)（OpenAPI 3.1）
- WebSocket 帧契约：[`docs/api/asyncapi.json`](asyncapi.json)（AsyncAPI）
- 错误码权威源：[`aux-03 错误码注册中心`](../templates/04-detailed-design/auxiliary/aux-03-error-code-registry.md) §B（**21 项**，与 `im_common::ErrorCode` 逐项一致）
- 协议帧样例：[`aux-13 协议帧样例`](../templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md)

> **本文的端点表由 `scripts/check-api-quickstart.ps1` 与 `openapi.json` 逐条对拍**，
> 改了一边忘了另一边，CI 会红。所以这里不会出现「文档说有、实际没有」的端点。

---

## 0. 五分钟走通一条消息

```bash
BASE=http://localhost:8080
ENV_ID=<你的 environment_id>          # 由平台运营方分配

# 1) 注册一个 guest 身份(无账号体系时最快)
curl -sX POST $BASE/v1/auth/guest \
  -H 'content-type: application/json' \
  -d "{\"environment_id\":\"$ENV_ID\",\"device_fingerprint\":\"demo-device-1\"}"
# -> {"user_id": "...", "access_token": "...", "refresh_token": "..."}

# 2) 建一个 DM 会话
curl -sX POST $BASE/v1/conversations \
  -H "authorization: Bearer $ACCESS_TOKEN" -H 'content-type: application/json' \
  -d '{"kind":"dm","member_user_ids":["<对方 user_id>"]}'

# 3) 发一条消息(idempotency_key 必须由客户端生成, 重复提交返回既有消息)
curl -sX POST $BASE/v1/conversations/<conversation_id>/messages \
  -H "authorization: Bearer $ACCESS_TOKEN" -H 'content-type: application/json' \
  -d '{"kind":"text","content":{"kind":"text","text":"hi"},"idempotency_key":"<uuid>"}'
```

WebSocket 在 `/v1/ws`（注意有 `/v1` 前缀；`wscat` 调试）：

```bash
wscat -c ws://localhost:8080/v1/ws
> {"type":"auth","req_id":"<uuid>","access_token":"<access_token>"}
< {"type":"auth_ok"}                      # 鉴权成功
> {"type":"send_message","req_id":"<uuid>","conversation_id":"<uuid>","idempotency_key":"<uuid>","kind":"text","content":{"kind":"text","text":"hi"}}
```

**本机起服务需要的环境变量**（`AppConfig` 里没有默认值的 4 个是硬性要求）：

| 变量 | 必需 | 说明 |
|---|---|---|
| `IM_POSTGRES_URL` | ✅ | PostgreSQL 连接串 |
| `IM_JWT_SIGNING_KEYS` | ✅ | JSON 数组，形如 `[{"kid":"v1","key":"...","active":true}]`；轮换期放 v1+v2 |
| `IM_REFRESH_PEPPER` | ✅ | Refresh Token 轮换用 |
| `IM_EVENT_PUBLISHER` | ✅ | `{"kind":"stub"}` 或 `{"kind":"nats","nats_url":"nats://..."}` |
| `IM_HTTP_PORT` | ➖ | 默认 `8080` |
| `IM_SERVER_SECRETS` | ➖ | S2S `token/exchange` 验签用；按 environment 存 secret |

---

## 1. 端点全表（24 个 operation）

| 分组 | Method | Path | 用途 |
|---|---|---|---|
| 运维 | `GET` | `/healthz` | 存活探针（不查依赖） |
| 运维 | `GET` | `/readyz` | 就绪探针：**会**查 PG 500ms 超时 + 读 NATS 连接状态；不可达返 `503` |
| 运维 | `GET` | `/metrics` | Prometheus 文本，10 个指标 |
| 鉴权 | `POST` | `/v1/auth/guest` | 注册访客（只需 `environment_id`） |
| 鉴权 | `POST` | `/v1/auth/token/exchange` | 游戏服 S2S 换 token（`external_provider` + `external_uid`） |
| 鉴权 | `POST` | `/v1/auth/refresh` | 刷新并轮换 |
| 鉴权 | `POST` | `/v1/auth/link` | guest → user 升级 |
| 鉴权 | `POST` | `/v1/auth/logout` | 吊销 device session（204） |
| 会话 | `POST` | `/v1/conversations` | 建会话（`kind` 只接受 `dm` / `group`） |
| 会话 | `GET` | `/v1/conversations` | 列当前用户的会话 |
| 会话 | `GET` | `/v1/conversations/{id}` | 单个会话详情 |
| 会话 | `GET` | `/v1/conversations/{id}/members` | 成员列表 |
| 会话 | `POST` | `/v1/conversations/{id}/read` | 上报已读（`last_read_sequence` 单调不回退） |
| 消息 | `POST` | `/v1/conversations/{id}/messages` | 发消息 |
| 消息 | `GET` | `/v1/conversations/{id}/messages` | 增量拉取（`?after_sequence=&limit=`） |
| 消息 | `PATCH` | `/v1/conversations/{id}/messages/{msg_id}` | 编辑 |
| 消息 | `POST` | `/v1/conversations/{id}/messages/{msg_id}/recall` | 撤回 |
| 消息 | `POST` | `/v1/conversations/{id}/messages/{msg_id}/reactions` | 加表情回应 |
| 好友 | `POST` | `/v1/friends/requests` | 申请（成功返 **204**） |
| 好友 | `POST` | `/v1/friends/requests/{id}/respond` | 接受 / 拒绝（**204**） |
| 好友 | `POST` | `/v1/friends/{id}/block` | 拉黑（**204**） |
| 资料 | `GET` | `/v1/me` | 当前用户资料 |
| 资料 | `PATCH` | `/v1/me` | 改展示名 |
| 实时 | `GET` | `/v1/ws` | WebSocket 升级 |

**所有**非 `ops` 端点都要 `authorization: Bearer <access_token>`。

---

## 2. 还没实现的（**接之前先看这段**）

这几个是**刻意不实装**的，`openapi.json` 里**没有**它们，请求会得到 `404`：

| 端点 | 为什么不实装 |
|---|---|
| `GET /v1/friends` | 规范冲突：`aux-13` 说返回 `repeated Friend`，`proto` 说 `repeated User`（后者等于把每个人的外部身份发给所有能列好友的人）。仓储层也缺 cursor 支持。**待规范所有者裁决** |
| `POST /v1/media/presign` | 预签名 URL 需要对象存储（MinIO/S3），该基础设施未落地。返 mock URL 比不实现更糟 |
| `GET /v1/media/{id}` | 同上 |

其它已知限制：

- **无速率限制**。`aux-08 §K GAP-8` 的 MVP 计划就是「Valkey 不可用即放行所有请求」，当前**根本没有限流**。请在接入侧自己做配额。
- **已读回执没有下行 WS 帧**。`mark_read` 成功即更新服务端状态，但**不会**推送给对端，对端需自己拉 `?after_sequence=`。
- **`message_new` 的 `reactions` 恒为 `[]`**（`hub.rs` 硬编码空数组）。刚发出的表情**不会**出现在随后某条消息的 `message_new` 里；实时到达请用 `reaction_added` 帧，或自己拉消息列表。
- **`auth_ok` 仍无规范定义**。2026-10-07 起它是 `ServerFrame::AuthOk` 枚举变体、纳入门禁与契约测试覆盖，但 `aux-13 §1.2` 仍未定义它（§1.2.1 定义的是 `connected`）。wire 形状**未变**，接入方不受影响；「它是否本应是 `connected` 的别名」待规范裁决。

> **2026-10-07 更新**：编辑与表情**已有实时同步**。`message_edited` /
> `reaction_added` 此前因 wire 形状不带 `conversation_id` 而无法定向投递
> （广播给所有人即跨会话泄漏），现已补上**必填单态** `conversation_id`，
> 与 `message_new` 走同一条「仅投递给会话成员」的路径。幂等重放的反应
> **不广播**，以免所有在线端看到同一个表情被重复动画一次。

---

## 3. WebSocket 帧

客户端 → 服务端 **8 类**：`auth` / `send_message` / `edit_message` / `recall_message` /
`react` / `mark_read` / `typing` / `ping`
服务端 → 客户端 **11 类**：`auth_ok` / `connected` / `ack`（成功/幂等冲突/错误 3 态）/ `message_new` /
`message_edited` / `message_recalled` / `reaction_added` / `presence_update` / `typing` /
`pong` / `force_disconnect`

其中生产代码**实际会发出** 8 类：`auth_ok` / `ack` / `message_new` / `message_edited` /
`message_recalled` / `reaction_added` / `typing` / `pong`。
`connected` / `presence_update` / `force_disconnect` 三类登记在协议里但**不会下发**
（`im-testkit` 的 mock server 会用 `connected`）。

完整形状见 [`aux-13`](../templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md) §1 与
[`asyncapi.json`](asyncapi.json)。

**握手阶段的错误码是带内帧，不是 HTTP 状态码** —— `/v1/ws` 的 `openapi.json` 声明里
只列了 `400`，因为握手阶段**唯一可能发生的 HTTP 失败就是 `400`（非 WS 握手请求）**；
鉴权失败等是连上以后以帧的形式回报。详见
[`gap-ledger §1.33`](../gap-ledger.md)。

---

## 4. 错误

统一形状，`Content-Type: application/json`：

```json
{ "error": { "code": "FRIEND_REQUEST_EXISTS", "message": "...", "details": {} } }
```

`code` 取自 **21 项**注册表（aux-03 §B）。`details` 在多数情况下为空 —— **服务端不会
把内部细节透出去**。HTTP 状态码与错误码**不是一一对应**的（两个不同的 `409` 可能
对应不同 `code`），所以**断言错误码，不要只断言状态码**。

---

## 5. 本文的状态（别把它当成实测记录）

本页的端点表与请求体形状**由脚本对拍 `openapi.json` 保证**，但**示例命令本身没有在
本机跑通过** —— 本机 Docker Desktop 不可用，起不来 PostgreSQL。因此：

- 路径、method、必填字段：**已核对**（脚本 + 读 spec）
- `curl` / `wscat` 示例：**未实测**，字段形状取自 spec，接入时请以你自己的实测为准

发现不一致请提 issue；`scripts/check-api-quickstart.ps1` 会挡住端点层面的漂移，但
挡不住示例语义写错。
