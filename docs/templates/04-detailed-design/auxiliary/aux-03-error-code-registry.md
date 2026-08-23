---
doc_id: aux-03
title_ja: エラーコードレジストリ (IM1.0)
title_zh: 错误码注册中心 (IM1.0)
phase: 04-detailed-design-aux
owners: Tech Lead + SRE
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 50 错误处理, 46 API 详细, 114 事件报告
---

# aux-03. エラーコードレジストリ (IM1.0) / 错误码注册中心 (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: Tech Lead + SRE
> 错误码源: `docs/DetailedDesign.md` §7 错误码表(MVP)

## 1. 目的 (Purpose)

为 IM1.0 全部错误码提供**唯一来源**、可申请、可查询、可弃用的中心化注册表。任何 PR 引入新错误码必须先在本表登记,CI 阻断未注册错误码。客户端基于 `error.code` 做分支处理(重试/降级/展示),不在业务层重新映射。

## 2. 适用范围 (Scope)

- 全部 REST 响应错误体中的 `code` 字段(`aux-13` §3 通用错误格式)
- 全部 WebSocket `ack.ok=false` 与 `error` 帧中的 `code` 字段
- 全部 gRPC 响应(经 `tonic::Status` 映射,本表中的 HTTP 状态列即 `tonic::Code` 对照)

## 3. 责任方 (Owners)

Tech Lead(定义 + 编码) + SRE(监控 + 告警 + 弃用)。新增错误码须 Tech Lead 同意;弃用须 Tech Lead + SRE 同意。

## 4. 前置依赖 (Prerequisites / Inputs)

- `docs/DetailedDesign.md` §7 错误码表(MVP 来源)
- `docs/50-error-handling-policy.md` 错误处理方针(模板, MVP 沿用 DetailedDesign §7 落地)
- 服务清单(BasicDesign §2)

## 5. 输出 / 模板正文 (Body)

## A. 错误码格式

**IM1.0 错误码采用 `UPPER_SNAKE_CASE`,**与本目录模板默认的 `{SERVICE}_{CATEGORY}_{NNN}` 略有不同 —— 经 Architecture Review 决定(详见 `DetailedDesign.md §7` 注释),IM1.0 客户端跨服务复用一个平铺命名空间,避免层级过深导致 wire size 膨胀;唯一性由本表集中保证而非格式前缀强制。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `code` | string | Y | 错误码,如 `UNAUTHORIZED` / `IDEMPOTENCY_CONFLICT` |
| `message` | string | Y | 人类可读描述(i18n key,**非最终用户文案**) |
| `http_status` | int | Y | HTTP 状态码(REST 路径) / 0 表示 WS 专用 |
| `grpc_code` | string | Y | 对应 `tonic::Code`(`OK` / `UNAUTHENTICATED` / `INVALID_ARGUMENT` 等) |
| `trace_id` | string | Y | 链路追踪 ID,用于服务端日志查询 |
| `ts` | int (unix ms) | Y | 错误发生时间(服务端) |

完整错误响应体格式(`aux-13` §3 通用格式):

```json
{
  "code": "UNAUTHORIZED",
  "message": "auth.token.expired",
  "trace_id": "tr_01HXY...",
  "ts": 1692528000000
}
```

## B. 错误码 → HTTP / gRPC 映射

下表为 **IM1.0 MVP 已注册** 的全部错误码,与 `DetailedDesign.md §7` 一一对应,任何 PR 增加必须同时更新本表与 DetailedDesign。

| 错误码 | HTTP | gRPC | 含义 | 触发条件 | 客户端建议处理 | 重试? |
|---|---|---|---|---|---|---|
| `UNAUTHORIZED` | 401 | `UNAUTHENTICATED` | Token 缺失 / 无效 / 过期 | 缺 Authorization 头 / 签名错误 / JWT exp 已过 | 触发 refresh(若 Access 过期)或重新登录 | 否 |
| `FORBIDDEN` | 403 | `PERMISSION_DENIED` | 无权限访问资源 | 资源存在但用户无权(如非会话成员) | 提示无权限,UI 隐藏入口 | 否 |
| `NOT_FOUND` | 404 | `NOT_FOUND` | 资源不存在 | ID 不存在 / 已删除 | 提示不存在,刷新本地缓存 | 否 |
| `IDEMPOTENCY_CONFLICT` | 200 (WS: ack.ok=true) | `OK` | 幂等键已处理,返回已有结果 | `(conversation_id, sender_id, idempotency_key)` 唯一索引命中 | **视为成功**(直接用 `data.message_id`) | 否(自动幂等) |
| `RATE_LIMITED` | 429 | `RESOURCE_EXHAUSTED` | 触发限流 | 单用户/单 IP 触发令牌桶上限 | 退避重试,读 `Retry-After` 头 | 是(指数退避) |
| `INVALID_STATE_TRANSITION` | 409 | `FAILED_PRECONDITION` | 状态机非法转换 | 如已 `recalled` 消息再次 `recall` | 刷新本地状态后再操作 | 否 |
| `RECALL_WINDOW_EXPIRED` | 409 | `FAILED_PRECONDITION` | 撤回时间窗已过 | `now() - message.created_at > environments.settings.message.recall_window_seconds` | 提示改用 delete(本地隐藏) | 否 |
| `ACCOUNT_BANNED` | 403 | `PERMISSION_DENIED` | 账号被封禁 | `users.state='banned'` | 展示封禁信息,跳登出 | 否 |
| `ACCOUNT_SUSPENDED` | 403 | `PERMISSION_DENIED` | 账号被暂停 | `users.state='suspended'` | 展示暂停信息 | 否(等风控复核) |
| `ACCOUNT_MERGE_CONFLICT` | 409 | `FAILED_PRECONDITION` | Guest Upgrade 目标身份已存在 | Link 时目标 external_identity 已绑定其他 user | 提示用户选择处理方式(MVP 不自动合并) | 否 |
| `FRIEND_REQUEST_EXISTS` | 409 | `FAILED_PRECONDITION` | 已存在待处理的好友申请 | `(env, sender, recipient)` uniq 命中且 state='pending' | 提示已发送申请,UI 标"等待中" | 否 |
| `USER_BLOCKED` | 403 | `PERMISSION_DENIED` | 目标用户在黑名单或被对方屏蔽 | `friendships.state='blocked'` | 提示无法发起操作 | 否 |
| `FRIEND_REQUEST_NOT_FOUND` | 404 | `NOT_FOUND` | 好友申请 ID 不存在或已被处理 | `friend_requests.id` 不存在 / 已被接受/拒绝/过期 | 提示申请不存在,刷新本地 | 否 |
| `VALIDATION_ERROR` | 400 | `INVALID_ARGUMENT` | 请求体校验失败 | 字段类型/长度/枚举值不合法 | 展示具体字段错误(后端响应含 `details: [{field, reason}]`) | 否 |
| `INTERNAL_ERROR` | 500 | `INTERNAL` | 服务端内部错误 | 兜底 catch-all | 重试(幂等操作)或提示稍后再试 | 是(指数退避,最多 3 次) |
| `SERVICE_UNAVAILABLE` | 503 | `UNAVAILABLE` | 依赖服务不可用 | PG / NATS / Valkey 暂时不可达 | 指数退避重试(2s/4s/8s,最多 5 次) | 是 |
| `CONVERSATION_NOT_FOUND` | 404 | `NOT_FOUND` | 会话不存在 | `conversations.id` 不存在或当前 user 非成员 | 提示会话不存在 | 否 |
| `MESSAGE_NOT_FOUND` | 404 | `NOT_FOUND` | 消息不存在 | `messages.id` 不存在或非当前会话 | 提示消息不存在 | 否 |
| `MESSAGE_TOO_LARGE` | 413 | `INVALID_ARGUMENT` | 消息体超过大小限制 | `messages.content` 序列化后 > 64KB(Candidate,见 DetailedDesign §10) | 提示消息过长,建议拆分 | 否 |
| `INVALID_IDEMPOTENCY_KEY` | 400 | `INVALID_ARGUMENT` | 幂等键格式非法 | 非 UUID / 超长 / 含非法字符 | SDK 自动修复(重新生成),仅调试场景出现 | 否 |
| `ENVIRONMENT_DISABLED` | 403 | `PERMISSION_DENIED` | 租户/环境已停用 | `environments.active=false` 预留(本表为 V1+) | 联系租户管理员 | 否 |

> **HTTP 200 特殊语义**:`IDEMPOTENCY_CONFLICT` 在 REST 路径实际返回 HTTP 200(响应体含原 `message_id`),WS 路径返回 `ack.ok=true, data={message_id}` —— 这是有意设计,客户端不需要分支处理"成功但用了缓存"。

## C. 错误码生命周期

```
申请中 → 已注册 → 已弃用
               ↓
            (重新激活)
```

### C.1 申请新错误码流程

1. 在本表"申请中"区加 1 行(PR 形式)
2. PR 评审:Tech Lead(必) + SRE(必)
3. 评审通过:分配正式码 → 进入"已注册"区
4. 代码 + 文档同步更新:
   - `crates/im-common/src/error.rs` 的 `ErrorCode` 枚举
   - 任何 `match` 全部穷尽(Clippy `match_str_case_mismatch` 阻断)
   - `aux-13` 协议帧样例(若新错误码影响 wire 格式)
5. 监控埋点:`error_code` 作为指标 label,接入 Grafana / SLO

### C.2 弃用错误码流程

1. 在"已弃用"区加 1 行(注明:何时弃用 / 替代码 / 客户端影响 / 灰度策略)
2. 保留 **6 个月** 兼容期,期间仍返回原错误码 + 新错误码(双发),监控调用量
3. 监控:`code=X` 6 个月内仍有调用 → 延长兼容期
4. 6 个月后 → 物理移除枚举 + CI 阻断新代码引用

## D. 已注册错误码(MVP 入口)

详见 §B 表格,共 **21 项** 错误码(2026-08-23 自审新增 `FRIEND_REQUEST_NOT_FOUND`)。所有 PR 引用本节错误码时必须以 `ErrorCode::Xxx` 枚举形式,**禁止**裸字符串。

## E. 申请中 / 已弃用

> 状态:截至 `2026-08-23`,无申请中 / 已弃用错误码。新增 / 弃用请按 §C 流程并在此节追加。

## F. 错误码 ↔ 日志 / 监控

| 层级 | 要求 |
|---|---|
| **应用日志** | 每个错误响应必带 `error_code` 字段(参见 `aux-09` 日志 cookbook) |
| **关键告警** | `INTERNAL_ERROR` / `SERVICE_UNAVAILABLE` / `ACCOUNT_BANNED` 自动接入 SRE Slack 告警(频率阈值后续在 `Observability.md` 定) |
| **客户端上报** | SDK 聚合 `code` × `count` → 上报到 `im-presence` 的 `telemetry` 主题(频次 Top N 监控,V1+ 启用,MVP 关闭) |
| **SRE 看板** | `error_code_rate{code}` 作为 SLO 面板的子图(详见 `Observability.md §6`) |

## G. 状态机(错误码生命周期)

参见 §C mermaid 流程。

## H. 编译期 / 运行期校验

| 阶段 | 校验手段 | 失败行为 |
|---|---|---|
| **编译期** | `ErrorCode` 枚举(`crates/im-common/src/error.rs`)由 `thiserror` derive,所有匹配用 `match` 穷尽 | Clippy `exhaustive_match` 阻断编译 |
| **CI** | `scripts/check_error_codes.sh` 扫描所有 `format!` / `to_string` 中的错误码字符串,确保都在本表 | CI 阻断 |
| **运行期** | 任何未注册的错误码字符串经过统一包装器 `ErrorCode::from_str` → 兜底为 `INTERNAL_ERROR` + 日志告警"unknown error code" | 不 panic,降级 |

## I. 客户端使用指引(SDK 行为)

```typescript
// TypeScript SDK 概念示例
async function handleResponse<T>(resp: Response): Promise<T> {
  if (resp.ok) return resp.json();
  const err: ErrorBody = await resp.json();
  switch (err.code) {
    case "UNAUTHORIZED":
      await refreshToken();
      return retry(resp);  // 内部自动重试 1 次
    case "RATE_LIMITED":
      const retryAfter = parseInt(resp.headers.get("Retry-After") ?? "1");
      await sleep(retryAfter * 1000);
      return retry(resp);
    case "IDEMPOTENCY_CONFLICT":
      // HTTP 200,不应进入此分支
      throw new Error("internal: IDEMPOTENCY_CONFLICT on 200");
    case "SERVICE_UNAVAILABLE":
    case "INTERNAL_ERROR":
      return retryWithBackoff(resp, { max: 3 });
    case "FORBIDDEN":
    case "NOT_FOUND":
    case "ACCOUNT_BANNED":
    case "ACCOUNT_SUSPENDED":
    case "USER_BLOCKED":
      throw new UserFacingError(err.code, err.message);
    case "VALIDATION_ERROR":
    case "INVALID_IDEMPOTENCY_KEY":
    case "MESSAGE_TOO_LARGE":
      throw new BugError(err.code, err.message);
    default:
      throw new UnknownError(err.code, err.message);
  }
}
```

## 6. 验收标准 (Acceptance Criteria)

- [ ] 所有错误码在本表登记(§B 已收 21 项 MVP 错误码,与 `crates/im-common/src/error.rs` `ErrorCode` 枚举严格对齐)
- [ ] CI 中 `scripts/check_error_codes.sh` 100% 通过
- [ ] 弃用 6 个月后未清零 → 自动告警(SRE 看板)
- [ ] 任何 `match err.code { ... }` 必有 `default` 分支(`@typescript-eslint/switch-exhaustiveness-check`)
- [ ] 客户端 SDK 覆盖本表全部错误码的分类处理(由 SDK 自身的单测覆盖)

## 7. 关联文档 (References)

- 关联工程活动: 50 错误处理, 46 API 详细, 114 事件报告
- 上游 Workflow: `docs/Workflow.md` Phase 4
- 上游: `docs/DetailedDesign.md` §7 错误码表
- 关联: `aux-09-log-query-cookbook.md` 错误码日志查询, `aux-13-protocol-frame-samples.md` 协议帧错误格式
- 关联: `Observability.md` §6 SLO 错误率指标

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-08-23 | Mavis 辅助 | 填实 IM1.0 错误码 20 项(§B);§A 解释为何用平铺命名而非模板默认的 `SERVICE_CATEGORY_NNN` 格式;§C 弃用 6 个月流程细化;§D 错误码 ↔ 日志/监控接入方式;§H 编译期/运行期/CI 三段校验;§I 客户端 SDK 错误码处理范例;§B 增加 IDEMPOTENCY_CONFLICT 在 REST 200 / WS ok=true 的特殊语义 |
