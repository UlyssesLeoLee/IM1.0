---
doc_id: aux-02
title_ja: データ辞書 (IM1.0)
title_zh: 数据字典 (IM1.0)
phase: 04-detailed-design-aux
owners: Tech Lead + DBA
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 15 数据要件, 31 ER 图, 47 DB 详细, 17 安全要件
---

# aux-02. データ辞書 (IM1.0) / 数据字典 (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: Tech Lead + DBA
> 适用: `docs/BasicDesign.md` 第4章列出的全部 PostgreSQL 表 + 对外 API 字段

## 1. 目的 (Purpose)

在 ER 之上,列出每个字段的详细属性(类型 / 范围 / 脱敏 / 隐私 / 引用方),供开发、测试、隐私审计、性能优化使用。本字典与 `BasicDesign.md §4` 的 DDL **必须 1:1 对应**,任何 schema 变更必须先更新本字典再改 DDL。

## 2. 适用范围 (Scope)

- 全部 PostgreSQL 表(`tenants` / `games` / `environments` / `users` / `device_sessions` / `friend_requests` / `friendships` / `conversations` / `conversation_sequences` / `dm_pairs` / `conversation_members` / `messages` / `message_reactions` / `audit_logs`)
- 全部 REST API 入参 / 出参字段
- 全部 WebSocket 帧字段(详细见 `aux-13-protocol-frame-samples.md`)

## 3. 责任方 (Owners)

Tech Lead(架构) + DBA(物理模型、性能、安全)。字段新增/删除/类型变更须两人之一 + PM 同意。

## 4. 前置依赖 (Prerequisites / Inputs)

- 概念 ER(`BasicDesign.md` §4)
- 数据需求(`SRS.md` §15 / IM-FR / IM-MSG / IM-REL / IM-CONV 系列)
- 隐私 / 合规要求(`SRS.md` §32, `SEC-NFR-006/007`)

## 5. 输出 / 模板正文 (Body)

## A. 字段属性清单(每字段必填)

| 属性 | 取值 |
|---|---|
| 物理名 | snake_case, 遵循 `aux-01` §D |
| 中文名 | 业务侧用语 |
| 类型 | PostgreSQL 类型(`UUID` / `BIGSERIAL` / `TIMESTAMPTZ` / `JSONB` / `TEXT` / `INTEGER` / `BOOLEAN` 等) |
| 长度 / 精度 | 适用时填写 |
| 必填 | Y / N |
| 默认值 | 适用时 |
| 取值范围 | 枚举 / 范围 / 正则 |
| 主键 | Y / N |
| 外键 | Y / N(指向 `表.字段`,on delete 行为) |
| 索引 | 索引名(若存在) |
| 唯一约束 | 唯一索引名(若存在) |
| 隐私级别 | 公开 / 内部 / 敏感 / 机密(见 §B) |
| 脱敏规则 | 展示 / 存储 / 日志三层策略 |
| 引用方 | 服务 / 模块 / API 端点 |
| 来源 | 用户输入 / 系统生成 / 同步自 X / 派生自 X |
| 留存期 | 永久 / N 天 / 法规要求 |
| 备注 | 关联 SRS ID / ADR / Candidate 标注 |

## B. 隐私级别

| 级别 | 含义 | 示例 | 脱敏要求 |
|---|---|---|---|
| 公开 | 可对外展示 | `display_name`, `conversation.metadata` 中的非敏感字段 | 无 |
| 内部 | 仅登录用户可见 | `user_id`, `conversation_id` | UI 不可逆 |
| 敏感 | 可见但需脱敏 | `email`, `phone`, `external_identity` | 部分 `*`(email 保留首字母,phone 保留后 4 位) |
| 机密 | 仅授权人可见 | `password_hash`, `server_secret`, `refresh_token_hash` | 全 `*`;日志中**绝不**打印 |

## C. 字段变更控制

| 变更类型 | 流程 |
|---|---|
| 字段新增 | 直接 `ALTER TABLE ADD COLUMN` + 立即更新本字典 |
| 字段重命名 | **必须**走"双写(新字段同步) → 数据迁移(后台 job) → 切流(应用读新) → 删除旧字段"4 步;期间保留双字段 7 天 |
| 字段删除 | **必须**先脱敏(变 NULL) + 通知业务 + 30 天观察期 |
| 类型变更 | **必须**先评估现有数据兼容性;大表(> 10M 行)用 online DDL(`pg_repack` / `pg_squeeze`) |
| 索引变更 | `CREATE INDEX CONCURRENTLY`(避免锁表),CI 中由 `aux-07` 校验 |
| 约束变更 | CHECK 约束变更前先验证现有数据满足新约束(脚本 + dry-run) |

## D. 枚举值定义

### `users.kind`

| 值 | 含义 | 触发 | 副作用 |
|---|---|---|---|
| `user` | 注册用户(已绑定 external identity) | Game Server Token Exchange | 标准能力 |
| `guest` | 临时访客(无 external identity) | `POST /v1/auth/guest` | 受限能力(受 `environments.settings.rate_limit.guest_register` 限流) |

### `users.state`(SRS IM-ID-004)

| 值 | 含义 | 触发 | 副作用 |
|---|---|---|---|
| `active` | 正常 | 默认 / 解封 | 无 |
| `banned` | 封禁 | 管理员操作 | 全部写操作 403,`ACCOUNT_BANNED` 错误 |
| `suspended` | 暂停 | 风控 | 写操作 403,`ACCOUNT_SUSPENDED` 错误 |
| `deleted` | 已注销(终态) | 用户自助 / 管理员 | 全部 API 404,`USER_NOT_FOUND` 错误 |

### `conversations.kind`(SRS IM-CONV-001)

| 值 | 含义 | 备注 |
|---|---|---|
| `dm` | 1:1 私聊 | 强制 `dm_pairs` 唯一对;成员数 = 2 |
| `group` | 群聊 | 任意成员数;含 sub-kind 在 metadata |
| `channel` | 频道(单向广播,只读) | 不可群发;SRS IM-CONV-003 明确不产生已读 |
| `system` | 系统会话(1 人) | 推送系统消息/审计/通知 |
| `broadcast` | 广播会话(临时,如 Match 直播) | 只读,V1+;MVP 不实现 UI |

### `messages.kind`(SRS IM-MSG-001)

| 值 | content JSON 必填字段 | 备注 |
|---|---|---|
| `text` | `text: string(max 4000)` | MVP 主类型 |
| `image` | `media_id: uuid, width?: int, height?: int, thumbnail_media_id?: uuid` | MVP |
| `file` | `media_id: uuid, file_name: string, size_bytes: int` | MVP |
| `sticker` | `sticker_id: string` | MVP(无表情包服务,仅做字段) |
| `system` | `event: string(join/leave/kicked/renamed/...)` | MVP(系统通知) |
| `custom` | `schema: string, data: object` | Extension 扩展用;Core 不解释 |

### `messages.state` / `messages.delivery_state`

详见 `aux-04-state-machine-spec.md` 与 `DetailedDesign.md §6.1`。本字典仅记字段约束:
- `state` ∈ {`sent`, `delivered`, `read`, `recalled`, `deleted`}
- `delivery_state` 仅作为客户端请求响应中的"当前已知投递状态"使用(冗余于 `state`,便于客户端显示)

### `friendships.state`

| 值 | 含义 |
|---|---|
| `accepted` | 互为好友 |
| `blocked` | A 拉黑 B(单向) |

### `audit_logs.action`(SRS SEC-NFR-006)

| 值 | 含义 |
|---|---|
| `auth.login` | 登录成功 |
| `auth.login_failed` | 登录失败 |
| `auth.token_exchange` | Server-to-Server Token 兑换 |
| `auth.token_refresh` | Refresh Token 刷新 |
| `auth.logout` | 主动登出 |
| `user.state_changed` | 状态变更(banned/suspended/deleted) |
| `secret_rotation` | 密钥轮换 |
| `extension.registered` | Extension 注册 |
| `extension.data_access` | Extension 访问数据 |
| `admin.action` | 管理操作(通用) |

## E. 跨实体引用关系

| 字段 | 源表 | → 目标表 | on delete | 备注 |
|---|---|---|---|---|
| `tenant_id` | `games` | `tenants` | RESTRICT | 租户删则游戏删(先删子) |
| `game_id` | `environments` | `games` | RESTRICT | 同上 |
| `environment_id` | `users` / `conversations` / `friend_requests` / `friendships` | `environments` | RESTRICT | 环境删则需级联清理(由应用层完成) |
| `user_id` | `device_sessions` / `friend_requests.sender_id` / `friendships.user_id` | `users` | CASCADE | 用户删则设备会话清 |
| `sender_id` / `recipient_id` | `friend_requests` | `users` | CASCADE | 同上 |
| `user_a` / `user_b` | `dm_pairs` | `users` | CASCADE | 用户删则 DM 关系清 |
| `conversation_id` | `conversation_sequences` / `dm_pairs` / `conversation_members` / `messages` / `message_reactions` | `conversations` | CASCADE | 会话删则全部关联清(审计可由 ETL 备份) |
| `sender_id` | `messages` | `users` | SET NULL | 系统消息 sender_id 必为 NULL;用户注销时消息保留但 sender 匿名 |
| `reply_to` | `messages` | `messages` | SET NULL | 回复目标删除时,仅清除引用,消息本身保留 |
| `message_id` | `message_reactions` | `messages` | CASCADE | 消息删则反应清 |
| `actor_id` | `audit_logs` | `users` | SET NULL | 用户注销后审计保留但 actor 匿名 |

## F. 字段详细表(每表 1 张)

### F.1 `tenants`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 租户 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 永久 | 全部 | |
| `name` | 租户名 | TEXT | Y | — | max 128 | | | 公开 | — | 永久 | 管理面 | |
| `created_at` | 创建时间 | TIMESTAMPTZ | Y | now() | — | | idx | 内部 | — | 永久 | 全部 | |

### F.2 `games`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 游戏 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 永久 | 全部 | |
| `tenant_id` | 所属租户 | UUID | Y | — | — | FK → `tenants.id` (RESTRICT) | idx | 内部 | — | 永久 | 全部 | |
| `name` | 游戏名 | TEXT | Y | — | max 128 | | uniq(tenant_id, name) | 公开 | — | 永久 | 管理面 | |

### F.3 `environments`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 环境 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 永久 | 全部 | |
| `game_id` | 所属游戏 | UUID | Y | — | — | FK → `games.id` (RESTRICT) | idx | 内部 | — | 永久 | 全部 | |
| `name` | 环境名 | TEXT | Y | — | {production, staging, test} | | uniq(game_id, name) | 内部 | — | 永久 | 全部 | |
| `settings` | 租户/环境级配置 | JSONB | Y | '{}' | 见 `BasicDesign §14.3` | | | 内部 | — | 永久 | im-core | 字段集见 `BasicDesign §14.3` 表格 |

### F.4 `users`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 用户 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 永久 | 全部 | |
| `environment_id` | 所属环境 | UUID | Y | — | — | FK → `environments.id` (RESTRICT) | idx | 内部 | — | 永久 | 全部 | |
| `kind` | 用户类型 | TEXT | Y | — | {user, guest} | | chk | 内部 | — | 永久 | identity | 枚举值见 §D |
| `external_identity` | 外部身份 | JSONB | N | NULL | `{provider: string, external_uid: string}` | | uniq(environment_id, external_identity) | 敏感 | 哈希展示 | 永久 | identity | Guest 必为 NULL;PG 默认 NULL 视为 distinct,允许多个 Guest 共存;User 必填且与 env 联合唯一 |
| `state` | 账户状态 | TEXT | Y | 'active' | {active, banned, suspended, deleted} | | chk | 内部 | — | 永久 | identity | 枚举值见 §D |
| `display_name` | 显示名 | TEXT | N | NULL | max 64 | | | 公开 | — | 永久 | identity | |
| `created_at` | 创建时间 | TIMESTAMPTZ | Y | now() | — | | idx | 内部 | — | 永久 | 全部 | |

### F.5 `device_sessions`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 设备会话 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 永久(直到 revoke) | auth | |
| `user_id` | 所属用户 | UUID | Y | — | — | FK → `users.id` (CASCADE) | idx | 内部 | — | 永久 | auth | |
| `device_fingerprint` | 设备指纹 | TEXT | N | NULL | max 256 | | | 内部 | 截断 hash | 90 天 | auth | 用于风控 |
| `refresh_token_hash` | Refresh Token 哈希 | TEXT | Y | — | bcrypt/argon2 output | | | 机密 | 全 `*` | 永久(直到 revoke) | auth | 永不打印 |
| `created_at` | 创建时间 | TIMESTAMPTZ | Y | now() | — | | idx | 内部 | — | 永久 | auth | |
| `revoked_at` | 撤销时间 | TIMESTAMPTZ | N | NULL | — | | | 内部 | — | 永久 | auth | NULL = 有效 |

### F.6 `friend_requests`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 申请 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 永久(到终态) | relationship | |
| `environment_id` | 所属环境 | UUID | Y | — | — | FK → `environments.id` (RESTRICT) | idx | 内部 | — | 永久 | relationship | |
| `sender_id` | 申请人 | UUID | Y | — | — | FK → `users.id` (CASCADE) | idx | 内部 | — | 永久 | relationship | |
| `recipient_id` | 接收人 | UUID | Y | — | — | FK → `users.id` (CASCADE) | idx | 内部 | — | 永久 | relationship | |
| `state` | 状态 | TEXT | Y | 'pending' | {pending, accepted, rejected, expired} | | chk | 内部 | — | 永久 | relationship | |
| `created_at` | 创建时间 | TIMESTAMPTZ | Y | now() | — | | idx | 内部 | — | 永久 | relationship | |
| `updated_at` | 更新时间 | TIMESTAMPTZ | Y | now() | — | | | 内部 | — | 永久 | relationship | 触发器自动更新 |
| — | — | — | — | — | — | | uniq(environment_id, sender_id, recipient_id) | — | — | — | — | 同一对不可重复申请 |

### F.7 `friendships`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `environment_id` | 所属环境 | UUID | Y | — | — | FK → `environments.id` (RESTRICT), PK(part) | PK | 内部 | — | 永久 | relationship | |
| `user_id` | 用户 | UUID | Y | — | — | FK → `users.id` (CASCADE), PK(part) | PK | 内部 | — | 永久 | relationship | |
| `friend_id` | 好友 | UUID | Y | — | — | FK → `users.id` (CASCADE), PK(part) | PK | 内部 | — | 永久 | relationship | |
| `state` | 关系状态 | TEXT | Y | 'accepted' | {accepted, blocked} | | chk | 内部 | — | 永久 | relationship | 枚举见 §D |
| `created_at` | 创建时间 | TIMESTAMPTZ | Y | now() | — | | | 内部 | — | 永久 | relationship | |

### F.8 `conversations`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 会话 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 永久 | 全部 | |
| `environment_id` | 所属环境 | UUID | Y | — | — | FK → `environments.id` (RESTRICT) | idx | 内部 | — | 永久 | 全部 | |
| `kind` | 会话类型 | TEXT | Y | — | {dm, group, channel, system, broadcast} | | chk | 内部 | — | 永久 | conversation | §D 枚举 |
| `metadata` | 业务元数据 | JSONB | Y | '{}' | namespaced, 见 SRS IM-CONV-002 | | | 内部(按 key) | — | 永久 | 全部 | **禁游戏专有字段**;仅 `game.*`/`ai.*`/`work.*` 命名空间 |
| `created_at` | 创建时间 | TIMESTAMPTZ | Y | now() | — | | idx | 内部 | — | 永久 | 全部 | |

### F.9 `conversation_sequences`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `conversation_id` | 所属会话 | UUID | Y | — | — | FK → `conversations.id` (CASCADE), PK | PK | 内部 | — | 永久 | message | |
| `next_sequence` | 下一个序号 | BIGINT | Y | 1 | ≥ 1 | | | 内部 | — | 永久 | message | `SELECT ... FOR UPDATE` 行锁,见 `DetailedDesign §5` |

### F.10 `dm_pairs`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `environment_id` | 所属环境 | UUID | Y | — | — | FK → `environments.id` (RESTRICT), PK(part) | PK | 内部 | — | 永久 | conversation | |
| `user_a` | 用户 A(较小) | UUID | Y | — | — | FK → `users.id` (CASCADE), PK(part) | PK | 内部 | — | 永久 | conversation | 强制 a < b |
| `user_b` | 用户 B(较大) | UUID | Y | — | — | FK → `users.id` (CASCADE), PK(part) | PK | 内部 | — | 永久 | conversation | 强制 a < b |
| `conversation_id` | DM 会话 | UUID | Y | — | — | FK → `conversations.id` (CASCADE) | uniq | 内部 | — | 永久 | conversation | |
| — | — | — | — | — | — | | chk(user_a < user_b) | — | — | — | — | CHECK 约束 |

### F.11 `conversation_members`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `conversation_id` | 所属会话 | UUID | Y | — | — | FK → `conversations.id` (CASCADE), PK(part) | PK | 内部 | — | 永久 | 全部 | |
| `user_id` | 成员用户 | UUID | Y | — | — | FK → `users.id` (CASCADE), PK(part) | PK + idx(user_id) | 内部 | — | 永久 | 全部 | user_id 独立索引支持"我的会话列表" |
| `role` | 角色 | TEXT | Y | 'member' | {owner, admin, member} | | chk | 内部 | — | 永久 | conversation | 后续可扩展 |
| `joined_at` | 加入时间 | TIMESTAMPTZ | Y | now() | — | | | 内部 | — | 永久 | conversation | |
| `last_read_sequence` | 已读 sequence | BIGINT | Y | 0 | ≥ 0 | | | 内部 | — | 永久 | conversation | 用于已读回执 |

### F.12 `messages`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 消息 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 永久 | 全部 | |
| `conversation_id` | 所属会话 | UUID | Y | — | — | FK → `conversations.id` (CASCADE) | idx(conversation_id, sequence) | 内部 | — | 永久 | 全部 | 核心查询路径 |
| `sequence` | 会话内序号 | BIGINT | Y | — | ≥ 1 | | uniq(conversation_id, sequence) | 内部 | — | 永久 | 全部 | 与 `idx` 复合 |
| `sender_id` | 发送者 | UUID | N | NULL | — | FK → `users.id` (SET NULL) | idx | 内部 | — | 永久 | 全部 | NULL = 系统消息 |
| `kind` | 消息类型 | TEXT | Y | — | {text, image, file, sticker, system, custom} | | chk | 内部 | — | 永久 | message | §D 枚举 |
| `content` | 消息内容 | JSONB | Y | — | 见 `aux-13 §4` | | | 内部(按 kind) | — | 永久 | 全部 | 不可信,展示时由前端 escape |
| `reply_to` | 回复目标 | UUID | N | NULL | — | FK → `messages.id` (SET NULL) | | 内部 | — | 永久 | message | |
| `idempotency_key` | 幂等键 | TEXT | Y | — | max 128, UUID 形态 | | uniq(conversation_id, sender_id, idempotency_key) NULLS NOT DISTINCT | 内部 | — | 永久 | message | PG 15+ 特性 |
| `state` | 消息状态 | TEXT | Y | 'sent' | {sent, delivered, read, recalled, deleted} | | chk | 内部 | — | 永久 | message | 状态机见 `aux-04` |
| `created_at` | 创建时间 | TIMESTAMPTZ | Y | now() | — | | idx | 内部 | — | 永久 | 全部 | |
| `edited_at` | 编辑时间 | TIMESTAMPTZ | N | NULL | — | | | 内部 | — | 永久 | message | |

### F.13 `message_reactions`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `message_id` | 消息 | UUID | Y | — | — | FK → `messages.id` (CASCADE), PK(part) | PK | 内部 | — | 永久 | message | |
| `user_id` | 反应者 | UUID | Y | — | — | FK → `users.id` (CASCADE), PK(part) | PK | 内部 | — | 永久 | message | |
| `emoji` | emoji | TEXT | Y | — | max 32 | | PK(part) | 公开 | — | 永久 | message | 允许非标准 emoji |

### F.14 `audit_logs`

| 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 | 引用方 | 备注 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `id` | 审计 ID | UUID | Y | gen_random_uuid() | — | PK | PK | 内部 | — | 法规要求(默认 1 年) | SRE / 合规 | |
| `tenant_id` | 租户 | UUID | Y | — | — | | idx(tenant_id, created_at DESC) | 内部 | — | 同上 | SRE | |
| `actor_id` | 行为人 | UUID | N | NULL | — | | | 内部 | 用户注销后置 NULL | 同上 | SRE | |
| `action` | 行为 | TEXT | Y | — | 见 §D audit_logs.action | | idx(action) | 内部 | — | 同上 | SRE | |
| `target_type` | 目标类型 | TEXT | Y | — | {user, conversation, environment, extension, secret} | | | 内部 | — | 同上 | SRE | |
| `target_id` | 目标 ID | UUID | N | NULL | — | | | 内部 | — | 同上 | SRE | |
| `detail` | 详情 | JSONB | N | NULL | — | | | 内部(按 key) | 敏感字段脱敏 | 同上 | SRE | 写入前过滤 `password`/`token`/`secret` |
| `created_at` | 创建时间 | TIMESTAMPTZ | Y | now() | — | | idx | 内部 | — | 同上 | SRE | |

## G. 隐私 / 合规映射

| 法规 | 涉及字段 | IM1.0 落地策略 |
|---|---|---|
| **GDPR** (欧盟) | `email`, `phone`, `external_identity`, `display_name` | 用户自助删除 → `users.state='deleted'`(30 天内完全清除) + ETL 同步清除派生数据 |
| **中国个人信息保护法** | `phone`, `external_identity` | 国内租户默认存储国内 region;出境需独立评估;单独同意机制由租户层配置 |
| **等保三级** | `password_hash`, `refresh_token_hash`, `server_secret` | 全部加密存储(详见 `SEC-NFR-007`,MVP 用 application-level 加密,V1+ 用 KMS) |
| **儿童隐私(COPPA 等)** | `users.created_at` 用于年龄推断 | 平台层不强制;租户可在 settings 启用额外校验(待 V1 落地) |

## 6. 验收标准 (Acceptance Criteria)

- [ ] 每张表上线前必须填完本字典对应子表
- [ ] 隐私字段 100% 标注脱敏规则(§A 必填项)
- [ ] 字段变更必须留 PR 链接 + 触发 CI 中的 schema diff 检查
- [ ] Core Schema(§F.4 `users` / §F.8 `conversations` / §F.12 `messages`)**不得**出现 `guild_id` / `match_id` / `party_id` 等游戏专有字段(由 `aux-01` §I + `aux-07` SQL 优化 checklist 联合校验)
- [ ] 所有 UUID 字段使用 `gen_random_uuid()` (PG 13+ `pgcrypto` 扩展或 PG 16+ 内置)
- [ ] `users.external_identity` UNIQUE 约束**不得**用 `NULLS NOT DISTINCT`(会阻断多个 Guest 共存;2026-08-23 自审发现的 BUG 已修,§F.4 注释说明 PG 默认 NULL distinct 行为)

## 7. 关联文档 (References)

- 关联工程活动: 15 数据要件, 31 ER 图, 47 DB 详细, 17 安全要件
- 上游 Workflow: `docs/Workflow.md` Phase 4
- 上游: `docs/BasicDesign.md` §4 (DDL 定义)
- 上游: `docs/SRS.md` §9 Domain Model, §15 数据需求, §32 隐私, §30 安全
- 关联: `aux-01-naming-convention.md` §D 数据库命名, `aux-04-state-machine-spec.md` 状态字段, `aux-07-sql-optimization-checklist.md` 索引/约束检查

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-08-23 | Mavis 辅助 | 填实 IM1.0 全部 14 张表(§F.1-F.14);§D 枚举值与 BasicDesign §4 严格对齐;§E on delete 行为按业务语义细化;§G 合规映射 GDPR/中国个保法/等保/COPPA;§C 字段变更控制补充 INDEX CONCURRENTLY;验收标准加 Core Schema 纯净性校验 |
