"""
gen_dd_aux.py
生成详细设计阶段(Phase 4)的 13 份辅助模板,放到 docs/templates/04-detailed-design/aux/
不在 Workflow 150 工程活动里,是 DD 阶段跨活动 / 跨切面的支撑性文档。

清单:
  aux-01-naming-convention.md          命名规范(跨所有代码 / DB / API)
  aux-02-data-dictionary.md            数据字典(比 ER 细:每个字段的隐私 / 脱敏)
  aux-03-error-code-registry.md        错误码注册中心(50 错误处理 的扩展)
  aux-04-state-machine-spec.md         业务对象状态机定义
  aux-05-crc-card.md                   CRC 卡(Class-Responsibility-Collaborator)
  aux-06-algorithm-performance-model.md 关键算法复杂度 / 性能模型
  aux-07-sql-optimization-checklist.md SQL 优化 Checklist
  aux-08-batch-retry-dlq.md            批处理重试 / 死信队列策略
  aux-09-log-query-cookbook.md         日志查询 Cookbook
  aux-10-dd-review-detailed-checklist.md  DD Review 详细 Checklist
  aux-11-key-sequence-diagrams.md      关键场景时序图集(mermaid)
  aux-12-config-spec.md                配置项规格(配置中心 / 环境变量)
  aux-13-protocol-frame-samples.md     协议帧样例集(WS / gRPC / REST)

使用:
  python scripts/gen_dd_aux.py
"""

from __future__ import annotations

from pathlib import Path
from textwrap import dedent

ROOT = Path("D:/IM1.0/docs/templates/04-detailed-design/aux")
PROJECT_NAME = "IM1.0"

PHASE_DIR = "04-detailed-design"
PHASE_LABEL = "詳細設計 / Detailed Design"
PHASE_SUBTITLE = "详细设计阶段辅助文档(非 150 工程活动,但 DD 阶段必备)"


# ---------------------------------------------------------------------------
# 渲染器
# ---------------------------------------------------------------------------

def render(doc_id: str, title_ja: str, title_zh: str,
           owners: str, purpose: str, scope: str, inputs: list[str],
           body: str, acceptance: str, related: list[str]) -> str:
    fm = f"""---
doc_id: {doc_id}
title_ja: {title_ja}
title_zh: {title_zh}
phase: {PHASE_DIR}-aux
owners: {owners}
status: Draft
version: 1.0.0
related_activities: {', '.join(related)}
---

# {doc_id}. {title_ja} / {title_zh}

> 项目: **{PROJECT_NAME}**
> 阶段: {PHASE_LABEL}(辅助)
> 责任方: {owners}

## 1. 目的 (Purpose)

{purpose}

## 2. 适用范围 (Scope)

{scope}

## 3. 责任方 (Owners)

{owners}

## 4. 前置依赖 (Prerequisites / Inputs)

{chr(10).join('- ' + x for x in inputs) if inputs else '- (无)'}

## 5. 输出 / 模板正文 (Body)

{body}

## 6. 验收标准 (Acceptance Criteria)

{acceptance}

## 7. 关联文档 (References)

- 关联工程活动: {', '.join(related)}
- 上游 Workflow: `docs/Workflow.md` Phase 4

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
"""
    return dedent(fm).lstrip("\n")


# ---------------------------------------------------------------------------
# 13 份模板内容
# ---------------------------------------------------------------------------

AUX: list[dict] = []

# -----------------------------------------------------------------------------
# 01. 命名规范
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-01",
    "filename": "aux-01-naming-convention.md",
    "title_ja": "命名規範",
    "title_zh": "命名规范",
    "owners": "Tech Lead",
    "purpose": "统一 IM1.0 全部代码、数据库、API、文档的命名规则,降低跨模块协作的认知负担。",
    "scope": "整个代码仓库(后端 Rust / 前端 TS / 协议 proto / DB / 制品 / CI / 文档)。",
    "inputs": ["语言 / 框架官方风格指南", "业务术语表(aux-02 / 业务侧词汇)"],
    "related": ["42 程序结构", "43 模块设计", "44 类设计", "46 API 详细", "47 DB 详细"],
    "body": dedent("""\
        ## A. 通用规则

        - **语言原生风格优先**:rust 用 clippy 推荐的 snake_case,TS 用 ESLint 推荐的 camelCase / PascalCase
        - **业务术语单语言**:用业务侧约定的中文/日文/英文,不混用
        - **避免缩写**:除非是行业通用(`api`, `id`, `url`, `db`),否则全词
        - **避免数字后缀**:`v1`, `v2` 只用于版本;不用于业务对象(`user1` / `user2` ❌)
        - **不与关键字冲突**:不与 `delete`, `class`, `type` 等重名

        ## B. Rust 命名

        | 类型 | 风格 | 示例 |
        |---|---|---|
        | 包 / 模块 | snake_case | `message_router` |
        | 类型 / Struct / Enum | PascalCase | `MessageEnvelope` |
        | Trait | PascalCase | `MessageStore` |
        | 函数 / 方法 | snake_case | `publish_message` |
        | 变量 | snake_case | `user_id` |
        | 常量 | SCREAMING_SNAKE_CASE | `MAX_MESSAGE_SIZE` |
        | 静态变量 | SCREAMING_SNAKE_CASE | `DEFAULT_PORT` |
        | 错误类型 | PascalCase + Error 后缀 | `PublishError` |
        | 特征实现 | `impl Trait for Type` | `impl Serialize for Message` |

        ## C. TypeScript / Next.js 命名

        | 类型 | 风格 | 示例 |
        |---|---|---|
        | 文件 / 目录 | kebab-case | `chat-panel.tsx` |
        | 组件 | PascalCase | `ChatPanel` |
        | Hook | camelCase + use 前缀 | `useChatRoom` |
        | 普通函数 | camelCase | `fetchMessages` |
        | 变量 | camelCase | `userId` |
        | 常量 | UPPER_SNAKE_CASE | `MAX_RETRY` |
        | 类型 / 接口 | PascalCase | `ChatMessage` |
        | 枚举值 | UPPER_SNAKE_CASE | `MessageStatus.SENT` |
        | 私有变量 | `_` 前缀 | `_internalState` |

        ## D. 数据库命名

        | 类型 | 风格 | 示例 |
        |---|---|---|
        | 表名 | snake_case + 复数 | `users`, `message_envelopes` |
        | 字段名 | snake_case | `user_id`, `created_at` |
        | 主键 | `id` | `id` |
        | 外键 | `<ref_table_singular>_id` | `user_id`, `room_id` |
        | 索引 | `idx_<table>_<columns>` | `idx_messages_room_id_created_at` |
        | 唯一索引 | `uniq_<table>_<columns>` | `uniq_users_email` |
        | CHECK 约束 | `chk_<table>_<column>` | `chk_users_age_positive` |
        | 触发器 | `trg_<table>_<action>` | `trg_users_before_insert` |

        ## E. API / 协议命名

        | 类型 | 风格 | 示例 |
        |---|---|---|
        | REST 路径 | kebab-case + 复数 | `/api/v1/chat-rooms` |
        | 查询参数 | snake_case | `?created_after=...` |
        | JSON 字段 | snake_case(API 内部) | `{"user_id": 1}` |
        | gRPC 服务 | PascalCase | `MessageService` |
        | gRPC 方法 | PascalCase + Verb | `PublishMessage` |
        | gRPC 消息 | PascalCase | `PublishMessageRequest` |
        | WebSocket 事件 | dot.case | `chat.message.received` |
        | 错误码 | UPPER_SNAKE_CASE | `AUTH_TOKEN_EXPIRED` |

        ## F. 文件 / 目录

        | 类型 | 风格 | 示例 |
        |---|---|---|
        | Rust 源文件 | snake_case | `message_router.rs` |
        | TS 文件 | kebab-case | `chat-panel.tsx` |
        | 目录 | snake_case 或 kebab-case | `services/chat-room/` |
        | 测试文件 | `<module>_test.rs` 或 `<module>.spec.ts` | |
        | 文档 | kebab-case | `workflow.md` |
        | 配置文件 | kebab-case | `docker-compose.yml` |

        ## G. 业务术语(单源)

        关键业务对象名必须在所有层(代码 / DB / API / 文档)保持一致:

        | 中文 | 英文 | 日文 | 说明 |
        |---|---|---|---|
        | 用户 | user | ユーザー | 平台账户 |
        | 消息 | message | メッセージ | 1 条 IM 消息 |
        | 房间 / 群 | room | ルーム | 1 对 1 或群聊 |
        | 频道 | channel | チャンネル | 直播 / 公开 |
        | 语音房间 | voice room | ボイスルーム | Voice 子系统 |
        | 凭证 | token | トークン | 身份令牌 |

        > 完整术语表见 `aux-02-data-dictionary.md` 附录。

        ## H. 禁止用法

        - ❌ 单字符变量名(除循环计数器 / lambda 参数)
        - ❌ 中文 / 日文作为标识符
        - ❌ 拼音命名(`yonghu`, `xiaoxi`)
        - ❌ 缩写 + 数字混合(`msg1`, `msg2`)
        - ❌ 与 Rust / TS 关键字同名
        - ❌ 在不同模块同一概念用不同名(必须走 G 列术语表)

        ## I. 工具强制

        - **Rust**:`cargo clippy -- -D clippy::all -W clippy::pedantic` 在 CI 阻断
        - **TS**:`eslint` + `@typescript-eslint/recommended`,CI 阻断
        - **DB**:`sqlfluff` 命名检查(自定义规则)
        - **API**:OpenAPI / proto lint 阻断
    """),
    "acceptance": "每名 PR 必须通过 clippy / eslint 命名检查;新表 / 新 API 必须先在术语表登记。",
})

# -----------------------------------------------------------------------------
# 02. 数据字典
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-02",
    "filename": "aux-02-data-dictionary.md",
    "title_ja": "データ辞書",
    "title_zh": "数据字典",
    "owners": "DB 設計者 + BA",
    "purpose": "在 ER 之上,列出每个字段的详细属性(类型 / 范围 / 脱敏 / 隐私 / 引用方),供开发、测试、隐私审计使用。",
    "scope": "全部数据库表 + 全部 API 入参 / 出参字段。",
    "inputs": ["概念 ER", "数据需求(15)", "隐私 / 合规要求"],
    "related": ["15 数据要件", "31 ER 图", "47 DB 详细", "17 安全要件"],
    "body": dedent("""\
        ## A. 字段属性清单

        每个字段必填:

        | 属性 | 取值 |
        |---|---|
        | 物理名 | snake_case |
        | 中文名 | (业务侧用语) |
        | 类型 | (PostgreSQL / 业务类型) |
        | 长度 / 精度 | |
        | 必填 | Y / N |
        | 默认值 | |
        | 取值范围 | (枚举 / 范围) |
        | 主键 | Y / N |
        | 外键 | Y / N(指向) |
        | 索引 | Y / N + 索引名 |
        | 隐私级别 | 公开 / 内部 / 敏感 / 机密 |
        | 脱敏规则 | (展示 / 存储 / 日志) |
        | 引用方 | (服务 / 模块 / 表) |
        | 来源 | (用户输入 / 系统生成 / 同步自 X) |
        | 留存期 | (永久 / N 天 / 法规) |

        ## B. 隐私级别

        | 级别 | 含义 | 示例 | 脱敏要求 |
        |---|---|---|---|
        | 公开 | 可对外展示 | nickname(可选) | 无 |
        | 内部 | 仅登录用户可见 | uid | UI 不可逆 |
        | 敏感 | 可见但需脱敏 | 手机号 | 中间 4 位 `*` |
        | 机密 | 仅授权人可见 | 身份证 | 仅保留地区码 |

        ## C. 模板(每个表 1 张)

        ### 表: `users`

        | 物理名 | 中文名 | 类型 | 必填 | 默认 | 范围 | 主/外键 | 索引 | 隐私 | 脱敏 | 留存 |
        |---|---|---|---|---|---|---|---|---|---|---|
        | id | 用户 ID | BIGSERIAL | Y | auto | — | PK | PK | 内部 | — | 永久 |
        | email | 邮箱 | VARCHAR(255) | Y | — | RFC 5322 | | uniq | 敏感 | 后 4 位 `*` | 永久 |
        | phone | 手机号 | VARCHAR(20) | N | — | E.164 | | uniq | 敏感 | 中 4 位 `*` | 永久 |
        | password_hash | 密码哈希 | VARCHAR(255) | Y | — | bcrypt/argon2 | | | 机密 | 全 `*` | 永久 |
        | nickname | 昵称 | VARCHAR(64) | N | — | — | | | 公开 | — | 永久 |
        | status | 状态 | SMALLINT | Y | 1 | 0=禁用 / 1=正常 / 2=禁言 | | idx | 内部 | — | 永久 |
        | created_at | 创建时间 | TIMESTAMPTZ | Y | now() | — | | idx | 内部 | — | 永久 |
        | updated_at | 更新时间 | TIMESTAMPTZ | Y | now() | — | | | 内部 | — | 永久 |

        (每个表 1 张子表)

        ## D. 枚举值定义

        ### 枚举: `user.status`

        | 值 | 中文 | 触发 | 副作用 |
        |---|---|---|---|
        | 0 | 禁用 | 管理员封禁 | 全部 token 失效 |
        | 1 | 正常 | — | — |
        | 2 | 禁言 | 管理员禁言 | 不能发消息,但能接收 |

        ## E. 跨实体引用关系

        | 字段 | 源表 | → 目标表 | 引用方式 |
        |---|---|---|---|
        | user_id | messages | users | RESTRICT(用户有消息不可删) |
        | room_id | messages | chat_rooms | CASCADE(房间删则消息删) |

        ## F. 字段变更控制

        - 字段新增:无需迁移,直接 ALTER
        - 字段重命名:必须走"双写 → 数据迁移 → 切流 → 删除旧字段"4 步
        - 字段删除:必须先脱敏(变 NULL) + 通知业务
        - 类型变更:必须先评估现有数据兼容性,大表用 online DDL(`pt-online-schema-change` / `pg_repack`)

        ## G. 隐私 / 合规映射

        - **等保三级**:敏感字段全部加密存储
        - **GDPR**:用户 ID / 设备 ID 可用于追踪;删除请求 30 天内生效
        - **中国个人信息保护法**:手机号 / 身份证 单独同意;出境单独评估
    """),
    "acceptance": "每张表上线前必须填完本字典;隐私字段 100% 标注脱敏规则;字段变更必须留 PR 链接。",
})

# -----------------------------------------------------------------------------
# 03. 错误码注册中心
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-03",
    "filename": "aux-03-error-code-registry.md",
    "title_ja": "エラーコードレジストリ",
    "title_zh": "错误码注册中心",
    "owners": "Tech Lead + SRE",
    "purpose": "为 IM1.0 全部错误码提供唯一来源、可申请、可查询、可弃用的中心化注册表;与 50 错误处理 一体。",
    "scope": "全部服务的错误码(REST / gRPC / WebSocket)。",
    "inputs": ["50 错误处理方针", "服务清单", "API 详细设计(46)"],
    "related": ["50 错误处理", "46 API 详细", "114 事件报告"],
    "body": dedent("""\
        ## A. 错误码格式

        ```
        {SERVICE}_{CATEGORY}_{NNN}

        示例: AUTH_TOKEN_EXPIRED
              MSG_PUBLISH_RATE_LIMITED
              DB_CONN_POOL_EXHAUSTED
        ```

        | 段 | 含义 | 范围 |
        |---|---|---|
        | SERVICE | 服务名 | AUTH / MSG / ROOM / VOICE / IM / SYS / DB |
        | CATEGORY | 类别 | TOKEN / PARAM / STATE / LIMIT / DOWN / UNKNOWN |
        | NNN | 序号 | 001-999 |

        ## B. 服务 / 类别范围

        | SERVICE | 范围 | 业务 |
        |---|---|---|
        | AUTH | 认证授权 | 登录 / Token / 权限 |
        | MSG | 消息 | 发送 / 接收 / 历史 |
        | ROOM | 房间 | 创建 / 加入 / 退出 |
        | VOICE | 语音 | 加入语音 / 推流 / 静音 |
        | IM | IM Core 通用 | 通用错误 |
        | SYS | 系统 | 内部错误 / 资源 |
        | DB | 数据库 | 连接 / 死锁 / 超时 |
        | EXT | 外部依赖 | 第三方失败 |

        | CATEGORY | 含义 | HTTP 类比 |
        |---|---|---|
        | TOKEN | 认证失败 | 401 |
        | PARAM | 参数错误 | 400 |
        | STATE | 状态非法 | 409 |
        | LIMIT | 限流 / 配额 | 429 |
        | DOWN | 服务不可用 | 503 |
        | UNKNOWN | 兜底 | 500 |

        ## C. 注册表(模板)

        | 错误码 | HTTP | 含义 | 触发条件 | 客户端行为 | 重试? | 引用 |
        |---|---|---|---|---|---|---|
        | AUTH_TOKEN_EXPIRED | 401 | Token 过期 | JWT exp < now | 跳登录页 | 否 | (文件) |
        | AUTH_TOKEN_INVALID | 401 | Token 无效 | 签名错误 | 跳登录页 | 否 | |
        | MSG_PUBLISH_RATE_LIMITED | 429 | 发送频率超限 | 60 条/分 | UI 提示 + 节流 | 是(1s) | |
        | MSG_NOT_FOUND | 404 | 消息不存在 | id 不在范围 | UI 隐藏 | 否 | |
        | ROOM_FULL | 409 | 房间已满 | 人数 = 上限 | 提示"房间已满" | 否 | |
        | VOICE_JOIN_FAILED | 503 | 加入语音失败 | SFU 不可用 | 提示重试 | 是(2s) | |
        | DB_CONN_POOL_EXHAUSTED | 503 | DB 连接池耗尽 | pool 全占用 | 提示"服务繁忙" | 是(指数退避) | |
        | SYS_INTERNAL_ERROR | 500 | 内部错误 | 兜底 | 提示"出错了" | 否 | |
        | EXT_DEPENDENCY_TIMEOUT | 504 | 外部依赖超时 | 5s 内未响应 | 降级 / 提示 | 是(指数退避) | |

        ## D. 申请 / 弃用流程

        ### 申请新错误码

        1. 在本表"申请中"区域加 1 行
        2. PR 评审:Tech Lead + SRE
        3. 评审通过:分配正式码 → 进入"已注册"区
        4. 代码 + 文档同步更新

        ### 弃用错误码

        1. 在"已弃用"区加 1 行(注明:何时弃用 / 替代码 / 客户端影响)
        2. 保留 6 个月的兼容期
        3. 监控:6 个月内仍有调用 → 延长

        ## E. 错误码 ↔ 日志 / 监控

        - 每个错误码必须有对应的日志字段 `error_code`
        - 关键错误码必须接入监控告警(SA / SEV)
        - 客户端上报:SDK 内部聚合 → 服务端入库 → 频次 Top N 监控

        ## F. 状态机(每个错误码生命周期)

        ```
        申请中 → 已注册 → 已弃用
                       ↓
                    (重新激活)
        ```

        ## G. 校验

        - 编译期:宏 / 生成器确保错误码字符串与枚举一致
        - CI:扫描代码,确保错误码都在本表注册
        - 运行期:未注册的错误码 → 兜底为 `SYS_UNKNOWN_ERROR`
    """),
    "acceptance": "所有错误码必须在本表登记;未登记错误码 CI 阻断;弃用 6 个月后未清零 → 自动告警。",
})

# -----------------------------------------------------------------------------
# 04. 状态机定义
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-04",
    "filename": "aux-04-state-machine-spec.md",
    "title_ja": "業務オブジェクト状態マシン仕様",
    "title_zh": "业务对象状态机定义",
    "owners": "BA + Tech Lead",
    "purpose": "为关键业务对象定义完整的状态机,覆盖所有合法 / 非法转换,作为编码和测试的唯一真源。",
    "scope": "用户、消息、房间、订单、订阅等关键业务对象。",
    "inputs": ["业务需求 BR(11)", "状态机相关用例"],
    "related": ["11 业务要件", "44 类设计", "50 错误处理"],
    "body": dedent("""\
        ## A. 业务对象清单

        | 对象 | 状态字段 | 状态数 | 关联表 |
        |---|---|---|---|
        | 用户(User) | status | 3 | users |
        | 消息(Message) | delivery_status | 4 | messages |
        | 房间(Room) | state | 4 | chat_rooms |
        | 语音房间(VoiceRoom) | state | 5 | voice_rooms |
        | 订阅(Subscription) | status | 3 | subscriptions |
        | 任务(Job) | state | 6 | jobs |

        ## B. 状态机规范

        ### B.1 消息投递状态 (Message.delivery_status)

        **状态值**:
        - `PENDING` — 已创建,等待发送
        - `SENT` — 已发送(到 IM Core)
        - `DELIVERED` — 已投递(到目标设备)
        - `READ` — 已读
        - `FAILED` — 失败(永久)

        **转换图** (mermaid):

        ```mermaid
        stateDiagram-v2
            [*] --> PENDING: 创建
            PENDING --> SENT: publish OK
            PENDING --> FAILED: publish FAILED(3 次重试)
            SENT --> DELIVERED: ack from receiver
            DELIVERED --> READ: read receipt
            FAILED --> [*]
            READ --> [*]
        ```

        **转换表**:

        | 源态 | 事件 | 目标态 | 守卫 | 副作用 |
        |---|---|---|---|---|
        | PENDING | publish_ok | SENT | 重试 ≤ 3 | 触发推送 |
        | PENDING | publish_fail | FAILED | 重试 > 3 | 记录失败原因 |
        | SENT | ack_received | DELIVERED | sender == receiver | 更新 read_at |
        | DELIVERED | read_receipt | READ | within 30d | 业务侧标记已读 |

        **不变量**:
        - 同一消息的状态只能向前 / 失败,不可回退
        - 超过 30 天的消息不可再更新

        ### B.2 房间状态 (Room.state)

        (类似结构)

        ### B.3 语音房间 (VoiceRoom.state)

        | 状态 | 含义 | 进入事件 | 退出事件 |
        |---|---|---|---|
        | `IDLE` | 空闲 | 创建 | first user join |
        | `ACTIVE` | 进行中 | first user join | last user leave |
        | `PAUSED` | 暂停(管理员) | admin pause | admin resume |
        | `CLOSED` | 关闭 | admin close | — |

        ## C. 通用规则

        - 所有状态转换必须**幂等**(同样的源态 + 同样的事件,结果一致)
        - 状态字段必须用**枚举类型**(类型系统阻断非法值)
        - 状态转换日志:记录 from / to / event / actor / time
        - 不可恢复的状态(如 `CLOSED`)是终态
        - 状态字段变更走数据库迁移 + 双写

        ## D. 状态机实现指引

        - **后端**:`enum` + 显式 `transition()` 函数,违反守卫返回错误码
        - **数据库**:`CHECK` 约束 + `ENUM` 类型
        - **缓存**:不缓存状态字段(避免不一致)
        - **事件**:每次状态变化发领域事件 → 事件总线

        ## E. 死锁 / 竞态处理

        - 用乐观锁:`version` 字段 + CAS
        - 用悲观锁:高竞争场景(语音房间加入)
        - 用分布式锁(Redis):跨服务状态协调

        ## F. 测试要求

        - 每个状态机必须有"全状态转换覆盖"测试
        - 反例测试:非法转换返回错误码
        - 死锁测试:并发状态变更不导致数据不一致
    """),
    "acceptance": "所有关键业务对象有完整状态机;非合法转换被类型系统 / 数据库 CHECK 阻断;状态变更日志可追溯。",
})

# -----------------------------------------------------------------------------
# 05. CRC 卡
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-05",
    "filename": "aux-05-crc-card.md",
    "title_ja": "CRC カード",
    "title_zh": "CRC 卡(Class-Responsibility-Collaborator)",
    "owners": "開発者",
    "purpose": "为每个核心类提供一张简明职责卡,聚焦做什么 / 不做什么 / 跟谁协作,作为代码评审的快速参照。",
    "scope": "所有 public 类(尤其领域模型 / 服务 / 仓储)。",
    "inputs": ["44 类设计", "模块设计(43)"],
    "related": ["44 类设计", "43 模块设计", "45 逻辑设计"],
    "body": dedent("""\
        ## 模板(每个类 1 张)

        ### CRC-{NNN} {ClassName}

        | 维度 | 内容 |
        |---|---|
        | 所在模块 | `services/{module}/` |
        | 类型 | 值对象 / 实体 / 聚合根 / 领域服务 / 应用服务 / 仓储 / 控制器 |
        | 关键字段 | (列出 5 个以内核心字段) |

        **职责 (Responsibilities)**

        - [ ] 职责 1
        - [ ] 职责 2
        - [ ] 职责 3

        **协作者 (Collaborators)**

        - → `OtherClass`:做什么事
        - ← `OtherClass`:接收什么

        **不在职责范围内 (Not responsible for)**

        - ❌ 不负责 X
        - ❌ 不负责 Y

        **关键方法 (Key Methods)**

        | 方法 | 入参 | 出参 | 复杂度 | 备注 |
        |---|---|---|---|---|
        | | | | | |

        **测试要点 (Test Points)**

        - 正常路径
        - 异常路径
        - 边界值
        - 并发

        **示例**

        ```rust
        // 公共签名
        ```

        ---

        ### CRC-001 MessageEnvelope

        | 维度 | 内容 |
        |---|---|
        | 所在模块 | `services/message/` |
        | 类型 | 值对象 |
        | 关键字段 | `id`, `room_id`, `sender_id`, `payload`, `created_at` |

        **职责**

        - [ ] 封装一条消息的全部属性
        - [ ] 校验 payload 大小 / 格式
        - [ ] 提供序列化 / 反序列化

        **协作者**

        - → `MessageRouter`:路由消息到目标房间
        - → `MessageStore`:持久化
        - ← `ChatController`:接收外部请求

        **不在职责范围内**

        - ❌ 不负责业务校验(由 `MessageService` 负责)
        - ❌ 不负责推送(由 `PushService` 负责)

        **关键方法**

        | 方法 | 入参 | 出参 | 复杂度 |
        |---|---|---|---|
        | `new` | `payload`, `sender` | `Self` | O(1) |
        | `serialize` | - | `Vec<u8>` | O(n) |
        | `deserialize` | `bytes` | `Self` | O(n) |

        **测试要点**

        - 正常:合法 payload 创建成功
        - 异常:超长 payload 拒绝
        - 边界:空 payload 行为
        - 并发:N/A(值对象不可变)
    """),
    "acceptance": "每个 public 类有 CRC 卡;不在职责范围内的事项写清楚;每张卡可 5 分钟内读完。",
})

# -----------------------------------------------------------------------------
# 06. 算法复杂度 / 性能模型
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-06",
    "filename": "aux-06-algorithm-performance-model.md",
    "title_ja": "アルゴリズム性能モデル",
    "title_zh": "关键算法复杂度 / 性能模型",
    "owners": "Tech Lead + SRE",
    "purpose": "为关键算法(路由 / 匹配 / 排序 / 搜索 / 加密)建立复杂度 + 性能基线,作为性能 NFR 的依据。",
    "scope": "性能敏感的所有算法(单条消息 ≤ 10ms 的路径)。",
    "inputs": ["NFR 性能(14)", "45 逻辑设计"],
    "related": ["14 NFR", "45 逻辑设计", "80 性能试验"],
    "body": dedent("""\
        ## A. 关键算法清单

        | 编号 | 算法 | 出现位置 | 输入规模 | 性能要求 |
        |---|---|---|---|---|
        | A-001 | 消息路由匹配 | im-router | N 房间 × M 成员 | < 5ms |
        | A-002 | 用户权限校验 | authz | 1 用户 × R 角色 | < 1ms |
        | A-003 | 在线状态广播 | presence | 1 万在线 | 广播 < 1s |
        | A-004 | 历史消息分页 | history | 1000 条 / 页 | < 50ms |
        | A-005 | 全文搜索(可选) | search | 1 亿消息 | < 200ms |

        ## B. 性能模型(模板)

        ### A-001 消息路由匹配

        **输入**:
        - 房间数 N
        - 房间平均成员 M
        - 1 条消息

        **算法**:
        - Step 1:查房间元数据(Redis):O(1)
        - Step 2:取成员列表(Redis):O(1)
        - Step 3:并发推送(长连接):O(M) 但并行

        **复杂度**:
        - 时间:O(1) + O(M / 并发度)
        - 空间:O(1)

        **性能模型**:
        - 单条消息延迟 = Redis 查(1ms) + 网络(2ms) + 推送握手(2ms) = ~5ms
        - 1 万成员 = 推送握手瓶颈 → 需分片

        **实测**:
        - (基准 / 平均 / P99 / 峰值)

        **优化策略**:
        - 1. Redis Pipeline 减少 RTT
        - 2. 长连接复用(同一连接多消息)
        - 3. 异步推送(不阻塞发件人)

        **NFR 链接**: NFR-P-001 (P99 端到端 < 200ms)

        ---

        (其他算法同样结构)

        ## C. 容量 / 性能 NFR 矩阵

        | 维度 | 目标 | 关联算法 |
        |---|---|---|
        | 端到端 P99 延迟 | < 200ms | A-001, A-002 |
        | 峰值 TPS | 10k | A-001 |
        | 在线用户 | 100 万 | A-003 |
        | 历史查询 P99 | < 50ms | A-004 |
        | 搜索 P99 | < 200ms | A-005(若有) |

        ## D. 实测 vs 目标

        | 算法 | 目标 | 实测平均 | 实测 P99 | 状态 |
        |---|---|---|---|---|
        | A-001 | < 5ms | 3ms | 8ms | ⚠ 需优化 |
        | A-002 | < 1ms | 0.5ms | 1.2ms | ✅ |

        ## E. 性能优化原则

        - 测量优先:不优化未测量的代码
        - 局部性:数据访问模式优先
        - 批处理:减少 RTT
        - 缓存:注意一致性
        - 异步:不阻塞关键路径

        ## F. 性能回退 / 复现

        - 性能回归测试:每个 PR 与 baseline 对比
        - 回归超阈值:PR 阻断
        - 性能基准:每周跑全量,看趋势
    """),
    "acceptance": "所有性能敏感算法有性能模型;实测有 baseline;性能回归 CI 阻断。",
})

# -----------------------------------------------------------------------------
# 07. SQL 优化 Checklist
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-07",
    "filename": "aux-07-sql-optimization-checklist.md",
    "title_ja": "SQL 最適化チェックリスト",
    "title_zh": "SQL 优化 Checklist",
    "owners": "DBA + 開発者",
    "purpose": "为关键 SQL 提供标准化的检查表,避免常见性能 / 正确性陷阱。",
    "scope": "生产环境运行的关键 SQL(单次执行 > 100ms 或日调用 > 1 万)。",
    "inputs": ["48 SQL 设计", "DB 详细(47)"],
    "related": ["47 DB 详细", "48 SQL 设计", "80 性能试验"],
    "body": dedent("""\
        ## A. 必查项(任何 SQL 上线前)

        - [ ] **避免 SELECT \\***:只查需要的列
        - [ ] **WHERE 用索引列**:不绕开(`WHERE LOWER(email) = ...` 走不上索引 → 改函数索引或归一化存储)
        - [ ] **JOIN 列类型一致**:避免隐式转换(数值 vs 字符串)
        - [ ] **LIMIT / OFFSET 合理**:深分页用 `WHERE id > ?` 替代
        - [ ] **COUNT 谨慎**:`COUNT(*)` 全表扫描;大表用 `EXISTS` 或近似
        - [ ] **子查询 vs JOIN**:相关子查询多次执行,优先 JOIN
        - [ ] **ORDER BY 索引列**:避免 filesort
        - [ ] **GROUP BY 用索引列**
        - [ ] **DISTINCT / UNION 去重**:检查是否有必要
        - [ ] **事务短小**:不持锁做计算
        - [ ] **预编译**:避免 SQL 注入 + 缓存执行计划

        ## B. 索引检查

        - [ ] 高频 WHERE 条件有索引
        - [ ] JOIN 关联列有索引
        - [ ] ORDER BY 列有索引
        - [ ] 复合索引最左前缀匹配
        - [ ] 索引选择性 > 5%(否则不如全表)
        - [ ] 不创建冗余索引
        - [ ] 不在小表上建索引(< 1k 行)

        ## C. 反模式(必须避免)

        | 反模式 | 原因 | 替代 |
        |---|---|---|
        | `SELECT *` | 带宽 + 隐式 IO | 显式列 |
        | `WHERE function(col) = ?` | 索引失效 | 函数索引或归一化 |
        | `LIKE '%xx%'` 前导通配 | 索引失效 | 全文索引 / ngram |
        | `OR col = ? OR col = ?` | 可能不走索引 | `IN (?, ?)` |
        | `NOT IN (SELECT ...)` | 难优化 | `NOT EXISTS` |
        | 大事务 | 锁竞争 | 分批 |
        | 隐式类型转换 | 索引失效 | 类型一致 |
        | `OFFSET 100000` 深分页 | 慢 | 游标 / 范围分页 |

        ## D. 性能阈值

        | SQL 类型 | 期望 |
        |---|---|
        | 主键点查 | < 1ms |
        | 索引范围 | < 10ms |
        | 简单聚合(全表) | < 100ms |
        | 复杂分析 | < 1s |
        | 报表 | < 5s(可异步) |

        ## E. 执行计划解读

        ```
        Seq Scan on users  (cost=0.00..1234.00 rows=10000)
          Filter: (status = 1)
        ```

        - ❌ Seq Scan:全表扫描,大表不可接受
        - ✅ Index Scan:走索引
        - ✅ Index Only Scan:覆盖索引,最优

        ## F. 慢查询处理流程

        1. **发现**:监控 / 用户反馈 / 慢日志
        2. **定位**:`EXPLAIN ANALYZE` 看执行计划
        3. **优化**:
           - 加索引
           - 改写 SQL
           - 拆分
           - 缓存
        4. **验证**:压测确认
        5. **回归**:加入性能基线

        ## G. 评审 Checklist

        - [ ] EXPLAIN 已附在 PR
        - [ ] 索引已加(若有)
        - [ ] 性能已测(基准 + 峰值)
        - [ ] 死锁 / 锁等待已分析
        - [ ] 资源消耗(CPU / IO)可接受
    """),
    "acceptance": "每条关键 SQL 上线前有 EXPLAIN + 评审;性能回归 CI 阻断。",
})

# -----------------------------------------------------------------------------
# 08. 批处理重试 / 死信队列
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-08",
    "filename": "aux-08-batch-retry-dlq.md",
    "title_ja": "バッチリトライ & デッドレター",
    "title_zh": "批处理重试 / 死信队列策略",
    "owners": "SRE + Tech Lead",
    "purpose": "为批处理作业统一重试 / 死信 / 幂等策略,避免无限重试和沉默失败。",
    "scope": "全部定时 / 事件触发 / 手动批处理作业。",
    "inputs": ["49 批处理详细", "50 错误处理"],
    "related": ["49 批处理详细", "50 错误处理", "32 批处理设计"],
    "body": dedent("""\
        ## A. 错误分类

        | 类别 | 示例 | 重试? |
        |---|---|---|
        | 瞬时错误 | 网络超时 / DB 死锁 / 限流 | 是 |
        | 持久错误 | 参数错误 / 业务规则违反 | 否,直接 DLQ |
        | 资源错误 | 磁盘满 / OOM | 是(等资源恢复) |
        | 外部错误 | 第三方 API 5xx | 是(退避) |
        | 数据错误 | 解析失败 / schema 不匹配 | 否,DLQ + 告警 |

        ## B. 重试策略

        ### B.1 通用参数

        | 参数 | 默认 | 备注 |
        |---|---|---|
        | 最大重试次数 | 3 | |
        | 初始退避 | 1s | |
        | 退避倍数 | 2 | 指数 |
        | 最大退避 | 60s | 防止间隔过大 |
        | 抖动 | ±20% | 防雪崩 |

        ### B.2 重试公式

        ```
        delay = min(initial * base^attempt, max_delay) * (1 ± jitter)
        attempt 1: 1s
        attempt 2: 2s
        attempt 3: 4s
        ...
        ```

        ## C. 死信队列 (DLQ)

        ### C.1 触发条件

        - 重试耗尽仍失败
        - 持久错误(不重试)
        - 数据错误(数据进入 DLQ 等待人工)

        ### C.2 死信结构

        ```
        {
          "id": "uuid",
          "original_task": "send_email",
          "payload": {...},
          "error": {
            "code": "EXT_SMTP_TIMEOUT",
            "message": "...",
            "stack": "..."
          },
          "failed_at": "2026-08-20T...",
          "attempts": 3
        }
        ```

        ### C.3 处理流程

        1. DLQ 入库 / 持久化
        2. 告警(企业微信 / 邮件)
        3. 运维 / 业务侧人工处置
        4. 重跑 / 跳过 / 永久丢弃(必须留记录)
        5. 复盘:同类型 1 周内 > N 次 → 自动 Jira

        ## D. 幂等保证

        ### D.1 必备属性

        - 同一作业**重复执行结果一致**
        - 同一作业**部分执行可恢复**

        ### D.2 实现

        - **业务幂等**:用业务键(如 `message_id`)做唯一约束
        - **状态机**:每个步骤记录状态,失败从断点继续
        - **去重表**:`processed_tasks(id, processed_at)`,执行前先 INSERT IGNORE
        - **乐观锁**:version 字段
        - **输出幂等**:写操作 upsert 而非 insert

        ## E. 监控 / 告警

        | 指标 | 阈值 | 告警 |
        |---|---|---|
        | 失败率 | > 5% | 通知 SRE |
        | DLQ 累积 | > 100 条 | 通知业务 |
        | 重试次数 > 2 | 每次 | 记录 |
        | 执行超时 | 超阈值 | 通知 |

        ## F. 重跑步骤模板(每作业)

        ```bash
        # 1. 检查作业状态
        ./jobctl status <job_name>

        # 2. (可选)重置检查点
        ./jobctl reset <job_name> --from <step>

        # 3. 重跑
        ./jobctl run <job_name> [--from <step>] [--dry-run]

        # 4. 验证
        ./jobctl verify <job_name>
        ```

        ## G. 案例

        ### JOB-001 用户日报生成

        - 失败:DB 临时不可用 → 瞬时错误
        - 行为:重试 3 次,指数退避
        - 失败:进入 DLQ,告警业务
        - 修复后:运维手动重跑,选 `--from step=2` 跳过已完成步骤
    """),
    "acceptance": "所有批处理有重试 + DLQ + 幂等保证;DLQ 24h 内有人工处置记录;失败率超阈值告警。",
})

# -----------------------------------------------------------------------------
# 09. 日志查询 Cookbook
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-09",
    "filename": "aux-09-log-query-cookbook.md",
    "title_ja": "ログクエリ Cookbook",
    "title_zh": "日志查询 Cookbook",
    "owners": "SRE",
    "purpose": "沉淀常见故障 / 性能问题的日志查询模式,新人 5 分钟内上手排障。",
    "scope": "生产 + staging 环境(查询语法基于 Loki / ELK / 平台特定)。",
    "inputs": ["51 日志设计", "38 监控设计"],
    "related": ["51 日志设计", "110 监控", "84 故障注入", "114 事件报告"],
    "body": dedent("""\
        ## A. 通用查询语法(Loki / LogQL 为例)

        ```
        {service="im-core"} |= "error" | json | latency_ms > 100
        {service="im-gateway"} |~ "AUTH_TOKEN_(EXPIRED|INVALID)"
        sum by (status) (rate({service="im-gateway"} |= "publish" [5m]))
        ```

        ## B. 常见场景查询

        ### B.1 找最近的错误

        ```
        {service="im-core"} |= "ERROR" | json | line_format "{{.msg}}"
        ```

        ### B.2 追踪单个用户

        ```
        {service=~"im-.*"} | json | user_id="12345" | line_format "{{.ts}} {{.service}} {{.msg}}"
        ```

        ### B.3 慢请求排查

        ```
        {service="im-gateway"} |= "publish" | json | latency_ms > 200
        ```

        ### B.4 限流命中

        ```
        {service=~".*"} |= "rate_limited" | json
        | sum by (user_id) (rate({...}[1m])) > 10
        ```

        ### B.5 错误码 Top 10

        ```
        sum by (error_code) (
          count_over_time({service=~"im-.*"} |= "ERROR" [1h])
        )
        ```

        ### B.6 跨服务链路

        ```
        {trace_id="abc123"}
        ```

        ## C. 故障模式 → 查询

        | 现象 | 第一步查询 | 进一步 |
        |---|---|---|
        | 客户端报错 5xx | `error_code=5*` | 看具体 `error_code` |
        | 延迟飙高 | `latency_ms > 200` by service | 定位慢服务 |
        | 消息丢失 | 查 `message_id` 全链路 | 验证是否发出 / 是否 ack |
        | 登录失败 | `AUTH_TOKEN_*` | 看是过期还是无效 |
        | 语音加入失败 | `VOICE_JOIN_FAILED` | 区分 SFU / 客户端 |
        | 资源告警 | 系统日志 | 看 CPU / 内存 / 连接 |

        ## D. 关键字段速查

        | 字段 | 含义 | 出现处 |
        |---|---|---|
        | `trace_id` | 链路追踪 ID | 所有服务 |
        | `user_id` | 用户 ID | 业务日志 |
        | `room_id` | 房间 ID | 业务日志 |
        | `error_code` | 错误码 | 错误日志 |
        | `latency_ms` | 耗时 | 慢日志 |
        | `msg_id` | 消息 ID | 消息日志 |

        ## E. 性能问题排查模板

        1. **定位时间点**:用户反馈 / 监控告警 → 找时间窗口
        2. **全局图**:错误率 / 延迟 / QPS 变化曲线
        3. **找异常服务**:`by (service)` 看哪个先降级
        4. **深挖**:慢查询 / 错误日志 / DB 锁
        5. **定位根因**:配置变更 / 流量尖峰 / 依赖故障
        6. **验证恢复**:对照修复前后曲线

        ## F. 必备仪表盘

        - **业务总览**:QPS / 错误率 / 在线用户
        - **服务总览**:每服务 P50 / P99 / 错误码 Top
        - **数据库**:QPS / 锁 / 慢查询
        - **依赖**:外部 API 状态

        ## G. 排障禁忌

        - ❌ 在生产环境 `cat` 大文件
        - ❌ 长时间开 debug 日志
        - ❌ 不带过滤的全量 grep
        - ❌ 直接删日志 / 重启服务(先评估)
    """),
    "acceptance": "每个 SRE 能用本 Cookbook 5 分钟内找到常见问题的根因;查询模式每季度更新。",
})

# -----------------------------------------------------------------------------
# 10. DD Review 详细 Checklist
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-10",
    "filename": "aux-10-dd-review-detailed-checklist.md",
    "title_ja": "詳細設計レビュー詳細チェックリスト",
    "title_zh": "详细设计评审详细 Checklist",
    "owners": "Tech Lead + Architect",
    "purpose": "为 52 DD Review 提供逐项可勾选的详细评审清单,确保评审有据可依。",
    "scope": "每次 DD Review 必用;按维度打勾并签字。",
    "inputs": ["42-51 全部详细设计文档", "本 checklist"],
    "related": ["52 DD Review", "41 BD Review", "56 代码评审"],
    "body": dedent("""\
        ## A. 完整性(每项 1 分)

        - [ ] 42 程序结构图覆盖全部代码
        - [ ] 43 模块设计每个模块有公共签名
        - [ ] 44 类图覆盖核心类
        - [ ] 45 关键逻辑有伪代码
        - [ ] 46 API 详细每个端点有字段表 + 错误码
        - [ ] 47 DB 详细 DDL 可执行
        - [ ] 48 关键 SQL 有 EXPLAIN
        - [ ] 49 批处理有步骤 / 失败处理
        - [ ] 50 错误处理策略齐全
        - [ ] 51 日志设计覆盖关键路径

        ## B. 一致性(每项 1 分)

        - [ ] 命名符合 aux-01
        - [ ] 错误码符合 aux-03
        - [ ] 状态机符合 aux-04
        - [ ] 与上游 BR / FR 可追溯
        - [ ] 与下游实现可对应

        ## C. 可追溯(每项 1 分)

        - [ ] 每个 API 端点可追溯到 FR
        - [ ] 每个表可追溯到数据要件
        - [ ] 每个错误码可追溯到错误处理
        - [ ] 每个类 / 模块可追溯到功能设计

        ## D. 可测试性(每项 1 分)

        - [ ] 每个公共方法有可测试边界
        - [ ] 外部依赖可 mock
        - [ ] DB 操作可集成测试
        - [ ] 性能敏感路径有性能模型
        - [ ] 状态机有转换测试用例

        ## E. 安全性(每项 1 分)

        - [ ] 认证 / 授权已设计
        - [ ] 敏感字段已识别 + 脱敏
        - [ ] 输入已校验
        - [ ] 注入 / XSS / CSRF 已防护
        - [ ] 密钥管理已规划

        ## F. 性能(每项 1 分)

        - [ ] 关键算法有复杂度分析
        - [ ] 慢 SQL 已识别 + 优化
        - [ ] 缓存策略已规划
        - [ ] 容量估算已完成
        - [ ] 性能 NFR 可达成

        ## G. 可维护性(每项 1 分)

        - [ ] 模块边界清晰
        - [ ] 依赖单向(无循环)
        - [ ] 公共 API 最小化
        - [ ] 文档自解释
        - [ ] 测试可独立运行

        ## H. 文档质量(每项 1 分)

        - [ ] 没有未填的 TBD
        - [ ] 图表清晰可读
        - [ ] 术语统一
        - [ ] 与编码规范一致
        - [ ] 变更可追溯(版本 / 修订人)

        ## I. 评分

        - **90-100 分**:通过
        - **70-89 分**:有条件通过(列出必须修复项)
        - **< 70 分**:不通过(重审)

        ## J. 评审结论

        | 维度 | 得分 | 备注 |
        |---|---|---|
        | 完整性 | /10 | |
        | 一致性 | /5 | |
        | 可追溯 | /5 | |
        | 可测试性 | /5 | |
        | 安全性 | /5 | |
        | 性能 | /5 | |
        | 可维护性 | /5 | |
        | 文档 | /5 | |
        | **总分** | **/45** | |

        ## K. 签字

        | 角色 | 签字 | 日期 |
        |---|---|---|
        | Tech Lead | | |
        | Architect | | |
        | 各模块 Owner | | |
        | 安全代表 | | |
    """),
    "acceptance": "总分 ≥ 38 才可通过;Critical 项缺失直接不通过;签字齐全。",
})

# -----------------------------------------------------------------------------
# 11. 关键场景时序图集
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-11",
    "filename": "aux-11-key-sequence-diagrams.md",
    "title_ja": "重要業務シナリオ時系列図集",
    "title_zh": "关键场景时序图集",
    "owners": "Architect",
    "purpose": "用 mermaid 时序图固化关键业务场景,作为跨团队沟通和代码评审的参照。",
    "scope": "核心业务 + 关键故障 / 异常路径。",
    "inputs": ["架构图(24)", "46 API 详细", "47 DB 详细"],
    "related": ["24 架构", "46 API 详细", "84 故障注入"],
    "body": dedent("""\
        ## 1. 用户发送消息(正常)

        ```mermaid
        sequenceDiagram
            participant C as Client
            participant G as im-gateway
            participant R as im-router
            participant DB as im-store
            participant S as im-sender

            C->>G: WS publish(msg)
            G->>R: route(msg)
            R->>DB: persist(msg)
            DB-->>R: ok
            R->>S: fanout(msg)
            S-->>C: WS message.received
            G-->>C: WS publish.ack
        ```

        ## 2. 用户加入语音房间

        ```mermaid
        sequenceDiagram
            participant C as Client
            participant G as im-gateway
            participant V as voice-svc
            participant S as LiveKit SFU

            C->>G: POST /voice/join
            G->>V: join_room(user, room)
            V->>S: create participant
            S-->>V: token, url
            V-->>G: token
            G-->>C: token
            C->>S: WebRTC connect
            S-->>C: media stream
        ```

        ## 3. 管理员封禁用户

        ```mermaid
        sequenceDiagram
            participant A as Admin
            participant G as im-gateway
            participant U as user-svc
            participant Cache as Redis
            participant N as im-sender

            A->>G: POST /admin/users/{id}/ban
            G->>U: ban(user_id)
            U->>U: update status = 0
            U->>Cache: invalidate token
            U->>N: user.banned event
            N-->>All: WS force_disconnect
            U-->>G: ok
            G-->>A: 200
        ```

        ## 4. 故障转移(im-gateway 主从切换)

        ```mermaid
        sequenceDiagram
            participant C as Client
            participant L as LB
            participant G1 as im-gateway-1(primary)
            participant G2 as im-gateway-2(standby)
            participant H as HealthCheck

            H->>G1: ping
            G1--xH: timeout
            H->>L: mark unhealthy
            L->>G2: promote
            G2->>G2: load state from Redis
            L-->>C: redirect to G2
            C->>G2: WS reconnect
        ```

        ## 5. 限流命中

        ```mermaid
        sequenceDiagram
            participant C as Client
            participant G as im-gateway
            participant R as RateLimiter
            participant Counter as Redis

            C->>G: WS publish(msg)
            G->>R: check(user_id, action)
            R->>Counter: incr
            Counter-->>R: count
            alt count > limit
                R-->>G: RATE_LIMITED
                G-->>C: error.ack
            else
                R-->>G: ok
                G->>G: process
            end
        ```

        ## 6. 数据库主从切换

        ```mermaid
        sequenceDiagram
            participant App as im-router
            participant P as PG Primary
            participant S as PG Standby
            participant L as LB

            App->>P: write
            P--xApp: timeout
            L->>S: detect primary down
            L->>S: promote to primary
            S->>S: replay WAL
            S-->>L: ready
            L-->>App: new primary
            App->>S: retry write
        ```

        ## 7. 历史消息分页

        ```mermaid
        sequenceDiagram
            participant C as Client
            participant G as im-gateway
            participant S as im-store
            participant DB as Postgres

            C->>G: GET /rooms/{id}/messages?before=msg_id
            G->>S: list before msg_id
            S->>DB: SELECT WHERE id < ? ORDER BY id DESC LIMIT 20
            DB-->>S: rows
            S-->>G: messages
            G-->>C: JSON
        ```

        ## 8. 房间扩容(成员超过阈值)

        ```mermaid
        sequenceDiagram
            participant A as Admin
            participant G as im-gateway
            participant R as im-router
            participant Cache as Redis

            A->>G: POST /rooms/{id}/scale
            G->>R: scale room (partition)
            R->>R: rebalance members
            R->>Cache: update routing key
            R-->>G: ok
            G-->>A: 200
        ```

        ---

        ## 维护

        - 新增场景:评审通过后追加
        - 旧场景:代码变更后必须更新对应图
        - 评审:每张图作为该场景编码的"必经"参照
    """),
    "acceptance": "每个核心业务场景有对应时序图;时序图与代码实现一致;新场景加入时需评审。",
})

# -----------------------------------------------------------------------------
# 12. 配置项规格
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-12",
    "filename": "aux-12-config-spec.md",
    "title_ja": "構成仕様書",
    "title_zh": "配置项规格",
    "owners": "SRE + Tech Lead",
    "purpose": "为所有运行期配置建立统一规格,涵盖分类 / 加载 / 热更新 / 密钥 / 环境差异。",
    "scope": "所有服务 + 客户端(可配置部分)。",
    "inputs": ["35 Infra 基本", "104 本番環境", "52 DD Review"],
    "related": ["35 Infra 基本", "53 开发环境", "104 本番环境"],
    "body": dedent("""\
        ## A. 配置分类

        | 类别 | 来源 | 加载时机 | 可热更新 | 示例 |
        |---|---|---|---|---|
        | 构建期 | 代码 / Cargo.toml | 编译 | ❌ | `version` |
        | 环境变量 | 容器 env | 启动 | ❌ | `DATABASE_URL` |
        | 配置中心 | Consul / Nacos | 启动 + 定期拉取 | ✅ | `rate_limit` |
        | 密钥管理 | Vault / KMS | 启动 | ✅(需重连) | `db_password` |
        | 特性开关 | 配置中心 | 启动 + 实时 | ✅ | `feature.voice.enabled` |

        ## B. 命名规范

        - snake_case + 点分级:`im.gateway.port`, `im.gateway.ws.timeout_ms`
        - 单位后缀:`_ms`(毫秒), `_s`(秒), `_bytes`(字节)
        - 范围后缀:`_min`, `_max`, `_default`

        ## C. 配置项清单(模板)

        ### 服务: `im-gateway`

        | 名称 | 类型 | 默认 | 范围 | 必填 | 类别 | 说明 |
        |---|---|---|---|---|---|---|
        | `im.gateway.http.port` | int | 8080 | 1024-65535 | Y | env | HTTP 端口 |
        | `im.gateway.ws.port` | int | 8081 | 1024-65535 | Y | env | WebSocket 端口 |
        | `im.gateway.ws.max_conn` | int | 100000 | 1-1000000 | Y | center | 最大连接数 |
        | `im.gateway.ws.timeout_ms` | int | 30000 | 1000-300000 | N | center | 心跳超时 |
        | `im.gateway.rate_limit.publish_per_min` | int | 60 | 1-1000 | Y | center | 发消息限流 |
        | `im.gateway.feature.voice_v2` | bool | false | — | N | center | 语音 v2 灰度 |
        | `im.gateway.db.url` | string | — | — | Y | vault | DB URL |
        | `im.gateway.db.password` | string | — | — | Y | vault | DB 密码 |

        ### 服务: `im-router`

        (同样结构)

        ## D. 环境差异

        | 配置 | dev | staging | prod |
        |---|---|---|---|
        | `*.log_level` | debug | info | warn |
        | `*.rate_limit.*` | 高(宽松) | 中 | 低(严格) |
        | `*.feature.*` | 全开 | 部分 | 默认 |
        | `*.db.*` | 本地 | 脱敏副本 | 生产 |
        | `*.external.*` | mock | 沙箱 | 真实 |

        ## E. 加载顺序

        1. 构建期配置(代码常量)
        2. 环境变量
        3. 配置中心 / 密钥管理
        4. 命令行参数(最高优先级)

        ## F. 热更新策略

        - **可热更**:限流阈值 / 特性开关 / 业务参数
        - **不可热更**:端口 / 数据库地址 / 密钥
        - **重载方式**:SIGHUP / 定期 poll(30s) / 主动推送
        - **一致性**:多实例下,使用 pub/sub 广播

        ## G. 密钥管理

        - 所有 `*_password`, `*_secret`, `*_key` 必须走 Vault / KMS
        - 不入配置中心;不写日志;不输出到前端
        - 轮换:DB 密码季度;API 密钥月度
        - 紧急轮换:有"立即作废"通道

        ## H. 校验

        - 启动时校验所有必填 + 范围
        - 不合法直接退出,不静默
        - 配置项变更要走 PR + 评审
        - 配置模板化:`config.example.yaml` 提交仓库,真实值不入库

        ## I. 监控

        - 配置项总数 / 变更次数
        - 热更新失败告警
        - 密钥轮换到期提醒
        - 未注册配置项使用告警(避免拼写错误)
    """),
    "acceptance": "所有服务有完整配置清单;密钥 100% 走 Vault;热更与启动加载分离;变更可审计。",
})

# -----------------------------------------------------------------------------
# 13. 协议帧样例集
# -----------------------------------------------------------------------------
AUX.append({
    "id": "aux-13",
    "filename": "aux-13-protocol-frame-samples.md",
    "title_ja": "プロトコルフレームサンプル集",
    "title_zh": "协议帧样例集",
    "owners": "Tech Lead",
    "purpose": "为 IM1.0 各协议(WS / gRPC / REST)提供完整可复制的帧样例,联调 / 排障直接对照。",
    "scope": "全部对外 + 对内协议。",
    "inputs": ["46 API 详细", "DetailedDesign.md §2-5"],
    "related": ["46 API 详细", "28 API 仕様", "29 IF 詳細"],
    "body": dedent("""\
        ## 1. WebSocket 帧

        ### 1.1 客户端 → 服务端:认证

        ```json
        {
          "op": "auth",
          "seq": 1,
          "body": {
            "token": "eyJhbGciOi..."
          }
        }
        ```

        ### 1.2 服务端 → 客户端:认证响应

        ```json
        {
          "op": "auth.ack",
          "seq": 1,
          "ts": 1692528000000,
          "body": {
            "ok": true,
            "user": {
              "id": "u_123",
              "nickname": "小明"
            }
          }
        }
        ```

        ### 1.3 客户端 → 服务端:发消息

        ```json
        {
          "op": "chat.publish",
          "seq": 42,
          "body": {
            "room_id": "r_abc",
            "type": "text",
            "payload": {
              "text": "你好"
            },
            "client_msg_id": "c_xyz"
          }
        }
        ```

        ### 1.4 服务端 → 客户端:发消息 ack

        ```json
        {
          "op": "chat.publish.ack",
          "seq": 42,
          "ts": 1692528000001,
          "body": {
            "ok": true,
            "msg_id": "m_001",
            "client_msg_id": "c_xyz"
          }
        }
        ```

        ### 1.5 服务端 → 客户端:收到新消息

        ```json
        {
          "op": "chat.message",
          "ts": 1692528001000,
          "body": {
            "msg_id": "m_001",
            "room_id": "r_abc",
            "sender": "u_456",
            "type": "text",
            "payload": {
              "text": "你好"
            },
            "created_at": 1692528000000
          }
        }
        ```

        ### 1.6 服务端 → 客户端:错误

        ```json
        {
          "op": "error",
          "seq": 42,
          "ts": 1692528001001,
          "body": {
            "code": "MSG_PUBLISH_RATE_LIMITED",
            "message": "Rate limit exceeded",
            "trace_id": "tr_abc"
          }
        }
        ```

        ## 2. gRPC 消息(im-proto)

        ### 2.1 PublishMessage

        ```protobuf
        service MessageService {
          rpc PublishMessage (PublishMessageRequest) returns (PublishMessageResponse);
        }

        message PublishMessageRequest {
          string room_id = 1;
          string sender_id = 2;
          MessagePayload payload = 3;
          string client_msg_id = 4;
        }

        message PublishMessageResponse {
          string msg_id = 1;
          int64 created_at = 2;
        }
        ```

        ### 2.2 JoinRoom

        ```protobuf
        rpc JoinRoom (JoinRoomRequest) returns (JoinRoomResponse);

        message JoinRoomRequest {
          string room_id = 1;
          string user_id = 2;
        }

        message JoinRoomResponse {
          repeated RoomMember members = 1;
          int64 joined_at = 2;
        }
        ```

        ## 3. REST API

        ### 3.1 创建房间

        **请求**:
        ```http
        POST /api/v1/rooms HTTP/1.1
        Host: api.example.com
        Authorization: Bearer eyJhbGciOi...
        Content-Type: application/json

        {
          "name": "项目讨论",
          "type": "group",
          "members": ["u_123", "u_456"]
        }
        ```

        **响应 (200)**:
        ```json
        {
          "id": "r_001",
          "name": "项目讨论",
          "type": "group",
          "created_at": 1692528000000,
          "members": ["u_123", "u_456"]
        }
        ```

        **错误 (400)**:
        ```json
        {
          "code": "PARAM_INVALID",
          "message": "Invalid type: must be one of [1-1, group, channel]",
          "trace_id": "tr_abc"
        }
        ```

        ### 3.2 错误响应(通用格式)

        ```json
        {
          "code": "AUTH_TOKEN_EXPIRED",
          "message": "Token expired",
          "trace_id": "tr_abc",
          "ts": 1692528000000
        }
        ```

        ## 4. 错误码样例(WS / REST 通用)

        | 错误码 | 场景 |
        |---|---|
        | `AUTH_TOKEN_EXPIRED` | Token 过期 |
        | `AUTH_TOKEN_INVALID` | Token 无效 |
        | `PARAM_INVALID` | 参数错误 |
        | `MSG_PUBLISH_RATE_LIMITED` | 发送频率超限 |
        | `MSG_NOT_FOUND` | 消息不存在 |
        | `ROOM_NOT_FOUND` | 房间不存在 |
        | `ROOM_FULL` | 房间已满 |
        | `VOICE_JOIN_FAILED` | 加入语音失败 |
        | `SYS_INTERNAL_ERROR` | 内部错误 |

        ## 5. 调试用 curl 样例

        ### 5.1 登录

        ```bash
        curl -X POST https://api.example.com/v1/auth/login \\
          -H "Content-Type: application/json" \\
          -d '{"email":"u@example.com","password":"secret"}'
        ```

        ### 5.2 发消息(REST)

        ```bash
        TOKEN=eyJhbGciOi...
        curl -X POST https://api.example.com/v1/rooms/r_001/messages \\
          -H "Authorization: Bearer $TOKEN" \\
          -H "Content-Type: application/json" \\
          -d '{"type":"text","payload":{"text":"hi"}}'
        ```

        ## 6. 协议版本

        - WS 协议版本:v1(在 `op` 字段加 `v1.` 前缀,例如 `v1.chat.publish`)
        - REST API 版本:`/v1/` 前缀
        - gRPC:`im.proto/v1`
        - 破坏性变更:必须升 v2,旧版保留 6 个月
    """),
    "acceptance": "每个协议端点有完整帧样例;样例与代码实现一致;协议变更时同步更新;调试 curl 可直接复用。",
})


# ---------------------------------------------------------------------------
# 写入
# ---------------------------------------------------------------------------

def main() -> int:
    print(f"[aux] 模板数: {len(AUX)}")
    print(f"[aux] 目标目录: {ROOT}")
    ROOT.mkdir(parents=True, exist_ok=True)
    for t in AUX:
        f = ROOT / t["filename"]
        f.write_text(render(
            t["id"], t["title_ja"], t["title_zh"],
            t["owners"], t["purpose"], t["scope"],
            t["inputs"], t["body"], t["acceptance"],
            t["related"]
        ), encoding="utf-8")
    # README
    readme = "# 详细设计阶段辅助模板\n\n"
    readme += "> 本目录是 `docs/templates/04-detailed-design/` 的扩展。\n"
    readme += "> 13 份文档不是 Workflow 150 工程活动的一部分,而是 DD 阶段跨活动 / 跨切面的支撑性文档。\n\n"
    readme += "## 清单\n\n"
    readme += "| 编号 | 名称 | 关联工程 | 责任方 |\n"
    readme += "|---|---|---|---|\n"
    for t in AUX:
        readme += f"| {t['id']} | {t['title_zh']} / {t['title_ja']} | {', '.join(t['related'])} | {t['owners']} |\n"
    readme += "\n## 使用\n\n"
    readme += "- 这些文档**与 42-52 主流程配合使用**;多数不是单次产出,而是持续维护的活文档。\n"
    readme += "- 上线时强制 100% 覆盖;PR 中显式引用相关 aux 文档。\n"
    readme += "- 重新生成:删除目录后运行 `python scripts\\gen_dd_aux.py`。\n"
    (ROOT / "README.md").write_text(readme, encoding="utf-8")
    print(f"[aux] 总文件数: {len(list(ROOT.rglob('*.md')))}")
    print("[ok] 全部 13 份辅助模板 + 1 README 已生成")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
