---
doc_id: aux-01
title_ja: 命名規範 (IM1.0)
title_zh: 命名规范 (IM1.0)
phase: 04-detailed-design-aux
owners: Tech Lead
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 42 程序结构, 43 模块设计, 44 类设计, 46 API 详细, 47 DB 详细
---

# aux-01. 命名規範 (IM1.0) / 命名规范 (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: Tech Lead
> 适用代码: `crates/im-{core,gateway,presence,media,protocol,common,store}/`

## 1. 目的 (Purpose)

统一 IM1.0 全部代码、数据库、API、协议的命名规则,降低跨服务协作认知负担与命名漂移风险。所有 PR 必须遵守本规范,Clippy / ESLint / sqlfluff 在 CI 阻断违规命名。

## 2. 适用范围 (Scope)

| 范围 | 包含 |
|---|---|
| 后端 | Rust crates (`im-*`), gRPC protobuf |
| 前端 | TypeScript / Next.js (Dashboard, HUD) |
| 数据库 | PostgreSQL 表、列、索引、约束、迁移文件名 |
| 协议 | WebSocket 帧、REST API 路径与字段、gRPC 方法 |
| 制品 | 容器镜像名 (`im1.0-im-gateway`)、K3s label、Helm chart 名 |
| 文档 | 文档文件名、章节号、ID 前缀 (`SRS` / `OBS` / `IM-FR-NNN` 等) |

## 3. 责任方 (Owners)

Tech Lead 主导定义 + 维护;SRE 监督制品/部署命名一致性;PM 监督文档命名一致性。

## 4. 前置依赖 (Prerequisites / Inputs)

- 语言/框架官方风格指南 (Rust API Guidelines / Google TS Style)
- 业务术语表(§G)
- `docs/SRS.md` §9 Domain Model

## 5. 输出 / 模板正文 (Body)

## A. 通用规则

- **语言原生风格优先**:Rust 用 clippy `naming` 模块推荐的 snake_case;TS 用 ESLint `@typescript-eslint/naming-convention` 推荐的 camelCase / PascalCase
- **业务术语单语言**:用业务侧约定的中文/日文/英文(IM1.0 用英文术语),不混用
- **避免缩写**:除非是行业通用(`id`, `url`, `db`, `ws`, `grpc`, `jwt`),否则全词
- **避免数字后缀**:`v1`, `v2` 只用于版本(API 版本、密钥版本);不用于业务对象(`user1` / `user2` ❌)
- **不与关键字冲突**:不与 `delete`, `class`, `type`, `match` 等重名
- **不引入业务外字段名**:Core Schema 不得出现游戏专有字段(Guild / Match / Party 等只能进 `metadata` JSONB 或 Extension 自有表,呼应 SRS `IM-CONV-002`)

## B. Rust 命名

| 类型 | 风格 | 示例 |
|---|---|---|
| 包 / 模块 | snake_case | `message_router`, `identity_service` |
| 类型 / Struct / Enum | PascalCase | `MessageEnvelope`, `ConversationKind` |
| Trait | PascalCase | `MessageRepository`, `EventPublisher` |
| 函数 / 方法 | snake_case | `publish_message`, `find_by_idempotency_key` |
| 变量 | snake_case | `user_id`, `conversation_id` |
| 常量 | SCREAMING_SNAKE_CASE | `MAX_MESSAGE_SIZE`, `DEFAULT_HEARTBEAT_SECONDS` |
| 静态变量 | SCREAMING_SNAKE_CASE | `DEFAULT_PORT` |
| 错误类型 | PascalCase + Error 后缀 | `PublishError`, `TokenValidationError` |
| 错误变体 | PascalCase,无后缀 | `PublishError::RateLimited` |
| 特征实现 | `impl Trait for Type` | `impl MessageRepository for PgMessageRepository` |
| 异步函数 | 同普通函数 | `pub async fn send_message(...)` |
| Builder 方法 | 同普通函数 | `with_capacity`, `for_environment` |

## C. TypeScript / Next.js 命名

| 类型 | 风格 | 示例 |
|---|---|---|
| 文件 / 目录 | kebab-case | `chat-panel.tsx`, `use-conversation.ts` |
| 组件 | PascalCase | `ChatPanel`, `MessageBubble` |
| Hook | camelCase + use 前缀 | `useChatRoom`, `useMessageStream` |
| 普通函数 | camelCase | `fetchMessages`, `parseJwtClaims` |
| 变量 | camelCase | `userId`, `conversationId` |
| 常量 | UPPER_SNAKE_CASE | `MAX_RETRY`, `DEFAULT_PAGE_SIZE` |
| 类型 / 接口 | PascalCase,无 `I` 前缀 | `ChatMessage`, `Conversation` |
| 枚举值 | UPPER_SNAKE_CASE | `MessageState.SENT` |
| 私有变量 | `_` 前缀 | `_internalState` |
| Props 类型 | `<ComponentName>Props` | `ChatPanelProps` |

## D. 数据库命名 (PostgreSQL)

| 类型 | 风格 | 示例 |
|---|---|---|
| 表名 | snake_case + 复数 | `users`, `messages`, `conversations` |
| 字段名 | snake_case | `user_id`, `created_at`, `sender_id` |
| 主键 | `id` (UUID / BIGSERIAL) | `id` |
| 外键 | `<ref_table_singular>_id` | `user_id`, `conversation_id`, `sender_id` |
| 索引 | `idx_<table>_<columns>` | `idx_messages_conversation_id_sequence` |
| 唯一索引 | `uniq_<table>_<columns>` | `uniq_users_environment_id_external_identity` |
| CHECK 约束 | `chk_<table>_<column>` | `chk_messages_state` |
| 触发器 | `trg_<table>_<action>` | `trg_messages_before_insert` |
| 迁移文件 | `<seq>_<description>.sql`(seq 为 4 位数) | `0001_create_tenants_games_environments.sql` |
| 枚举类型 | snake_case | `conversation_kind`, `user_state` |

## E. API / 协议命名

| 类型 | 风格 | 示例 |
|---|---|---|
| REST 路径前缀 | `/v{major}/` | `/v1/`, `/v2/` |
| REST 资源 | 复数 + kebab-case 复合 | `/v1/conversations`, `/v1/media-presign` |
| REST 资源子资源 | `/v1/{resource}/{id}/{sub-resource}` | `/v1/conversations/{id}/messages` |
| 查询参数 | snake_case | `?after_sequence=42&limit=20` |
| JSON 字段 (WS 帧) | snake_case | `{"conversation_id": "...", "idempotency_key": "..."}` |
| JSON 字段 (REST body) | snake_case | `{"display_name": "小明"}` |
| gRPC package | `im.<service>.v<major>` | `im.core.v1`, `im.media.v1` |
| gRPC 服务 | PascalCase | `CoreService`, `MediaService` |
| gRPC 方法 | PascalCase + Verb | `PublishMessage`, `ListMessages` |
| gRPC 消息 | PascalCase + Request/Response | `PublishMessageRequest` |
| WebSocket 帧 type | dot.case | `send_message`, `message_new`, `presence_update` |
| 错误码 (REST/WS 通用) | UPPER_SNAKE_CASE | `UNAUTHORIZED`, `IDEMPOTENCY_CONFLICT`, `RATE_LIMITED` |
| HTTP Header | Pascal-Case (HTTP/2) | `X-Request-Id`, `Authorization` |

## F. 文件 / 目录

| 类型 | 风格 | 示例 |
|---|---|---|
| Rust 源文件 | snake_case | `message_router.rs` |
| Rust 测试文件 | 同名 + `_test.rs` 或 `tests/` 子目录 | `message_router_test.rs` |
| TS 文件 | kebab-case | `chat-panel.tsx` |
| 目录 | snake_case (Rust) / kebab-case (TS) | `crates/im-core/src/identity/` |
| 文档 | kebab-case (英文) / 全中文 | `day1-task-list.md` 或 `Day1-Task-List.md` |
| 配置文件 | kebab-case | `docker-compose.yml`, `Cargo.toml` |
| Proto 文件 | snake_case | `core.proto`, `media.proto` |

## G. 业务术语(单源) —— IM1.0 专用

关键业务对象名必须在所有层(代码 / DB / API / 文档)保持一致。**禁止**用其它同义词替换。

| 中文 | 英文 (单源) | 日文 | 说明 | 严禁同义词 |
|---|---|---|---|---|
| 租户 | `tenant` | テナント | 平台客户(发行商/工作室) | ~~org, organization, account~~ |
| 游戏 / 应用 | `game` | ゲーム | 租户下的具体游戏 | ~~app, application, product~~ |
| 环境 | `environment` | 環境 | production / staging / test | ~~env, stage, deploy~~ |
| 用户 | `user` | ユーザー | 平台账户(User Kind = 'user') | ~~account, member, person~~ |
| 访客 | `guest` | ゲスト | User Kind = 'guest'(临时身份) | ~~visitor, anonymous~~ |
| 设备会话 | `device_session` | デバイスセッション | Refresh Token 绑定单元 | ~~device, session, client~~ |
| 令牌 | `token` | トークン | 身份令牌(Access / Refresh / Server) | ~~credential, secret~~ |
| 关系 | `relationship` | リレーション | 好友/拉黑/关注 | ~~friendship, social~~ |
| 会话 | `conversation` | 会話 | DM/Group/Channel/System/Broadcast | ~~room, channel, chat, group, thread~~ ❌ |
| 消息 | `message` | メッセージ | 1 条 IM 消息 | ~~msg, post, entry~~ |
| 投递状态 | `delivery_state` | 配信状態 | sent/delivered/read/recalled/deleted | ~~status, state, ack~~(后者保留给 User.state) |
| 在线状态 | `presence` | プレゼンス | online/offline/away/busy | ~~status, online~~ |
| 反应 | `reaction` | リアクション | 消息表情回应 | ~~emoji, like, ack~~ |
| 媒体对象 | `media_object` | メディア | 上传到 MinIO 的文件 | ~~file, attachment, asset~~ |
| 扩展 | `extension` | 拡張 | 通过 Extension Runtime 接入的能力 | ~~plugin, addon, module~~ |
| 能力 | `capability` | ケイパビリティ | Extension 声明可调用的能力 | ~~permission, scope~~ |
| 事件总线主题 | `im.<aggregate>.<verb_past>` | — | `im.message.created` | ~~topic, channel, queue~~ |
| 顺序号 | `sequence` | シーケンス | Conversation 内消息单调递增序号 | ~~seq, sn, order, offset~~ |

> **Critical Rule**:`room` / `channel`(作为 IM 业务对象) / `chat` 在 IM1.0 Core 中**严禁使用**——本项目用 `conversation` 统一表达。
> - 例外:`#voice-channel` 是 LiveKit 专有概念(其 `Room` 在 §G 不映射),`Channel` 仅用于 `conversations.kind='channel'` 这一种枚举值,不作为对象名
> - 例外:Slack/Discord 风格的 "channel" 在 IM1.0 中对应 `conversations.kind='channel'`(命名空间广播),不是新对象

## H. 协议包与版本

| 项 | 命名 | 升级规则 |
|---|---|---|
| WebSocket 协议版本 | 帧 `type` 字段加 `v1.` 前缀(如 `v1.send_message`)(候选,见 `aux-13`) | 不兼容变更升 v2,旧版保留 6 个月 |
| REST API 版本 | URL 路径 `/v1/`,`/v2/` | 不兼容变更升 v2,旧版保留 6 个月 |
| gRPC | package `im.<svc>.v1` | 同上 |
| DB 迁移 | 顺序号 + 描述 | 永远追加,不改历史 |

## I. 禁止用法

- ❌ 单字符变量名(除循环计数器 / lambda 参数)
- ❌ 中文 / 日文作为标识符
- ❌ 拼音命名(`yonghu`, `xiaoxi`)
- ❌ 缩写 + 数字混合(`msg1`, `msg2`)
- ❌ 与 Rust / TS / SQL 关键字同名
- ❌ 在不同模块同一概念用不同名(必须走 §G 术语表)
- ❌ 在 `conversations`/`messages` 等 Core 表加游戏专有字段(违反 `IM-CONV-002`)
- ❌ 用 `room` / `chat` 命名 IM 业务对象(必须用 `conversation`)
- ❌ 用 `status` 命名"投递状态"(必须用 `delivery_state`),`status` 仅限 `users.state`

## J. 工具强制

| 层 | 工具 | CI 阻断规则 |
|---|---|---|
| Rust | `cargo clippy -- -D clippy::all -D clippy::pedantic -D clippy::naming` | 命名违规 → CI 失败 |
| TypeScript | `eslint` + `@typescript-eslint/recommended` + naming plugin | 命名违规 → CI 失败 |
| DB | `sqlfluff` 自定义命名规则(`L030` 等) + `cargo sqlx prepare` 检查 schema drift | 命名违规 → CI 失败 |
| gRPC | `buf lint` (breaking change check) | 不兼容变更 → CI 失败 |
| OpenAPI | `openapi-spec-validator` | 规范违规 → CI 失败 |
| 文档 | 人工 review(PR 中 `aux-01` 影响项必填自评) | 命名违规 → review 驳回 |

## 6. 验收标准 (Acceptance Criteria)

- [ ] 每个 PR 通过 clippy / eslint / sqlfluff 命名检查
- [ ] 新表 / 新 API / 新协议字段必须先在 §G 术语表登记
- [ ] 任何对 §G 术语的偏离必须提交 ADR 说明理由,且不引入新同义词
- [ ] §I 禁止用法在 CI 中有对应检查项,违规阻断合并
- [ ] `conversations` / `messages` 等 Core 表结构不出现 `guild_id` / `match_id` 等游戏专有字段(由 `aux-07` SQL 优化 checklist 中的"领域纯净性"项把关)

## 7. 关联文档 (References)

- 关联工程活动: 42 程序结构, 43 模块设计, 44 类设计, 46 API 详细, 47 DB 详细
- 上游 Workflow: `docs/Workflow.md` Phase 4
- 相关: `aux-02-data-dictionary.md` §D 枚举值定义, `aux-13-protocol-frame-samples.md` 协议版本规则

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-08-23 | Mavis 辅助 | 填实 IM1.0 专用术语表(§G);补强 §D 迁移文件名;§E gRPC package 命名;§H 协议版本规则;§I 增加"Core Schema 禁游戏专有字段"红线;§J 工具强制列出 sqlfluff / buf / openapi-spec-validator |
