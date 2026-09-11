---
doc_id: B-3
title_zh: friend_requests UNIQUE 跨 state 重发策略 — 决策选项 + 推荐
phase: 15-management
wbs_id: B-3
status: Awaiting PM (Ulysses) decision
version: 1.0.0
date: 2026-09-01
---

# B-3. `friend_requests` UNIQUE 跨 state 重发策略 — 决策选项

> 项目: **IM1.0**
> WBS ID: **B-3** (per `docs/132-wbs.md` v1.0.0 §5.2)
> 责任方 (拍板): **PM (Ulysses)** — 本文档不替 PM 决策,仅给 3 选项 + 实施约束 + 推荐
> 实施约束承接: `docs/ImplementationSpec.md` v1.0.3 §16 P2-1 + `migrations/0003_create_friend_requests_and_friendships.sql` 头部 2026-08-23 自审注
> 关联: 132-wbs B-3 (P2-1) → C-1 (relationship repository) lag = -1d → C-7 (auth/link) → D-2

## 1. 背景 (Context)

`migrations/0003_create_friend_requests_and_friendships.sql` 当前定义:

```sql
CREATE TABLE IF NOT EXISTS friend_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    sender_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'accepted', 'rejected', 'expired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (environment_id, sender_id, recipient_id),  -- ← 跨 state 锁死
    CHECK (sender_id <> recipient_id)
);
```

`UNIQUE (environment_id, sender_id, recipient_id)` 是**全表唯一**,不分 state。
即:`(env, alice, bob)` 一旦存在一条记录(无论 pending/accepted/rejected/expired),alice 再发申请就会触发 23505 unique_violation。

**P2-1 已知局限**(ImplementationSpec §16):多数 IM 产品被拒后必须走"重新申请"按钮(用户主动重发),但部分产品(微信/QQ 早期)允许"被拒后自动重发"。需要在 C-1 (relationship repository) 编码前 PM 拍板。

## 2. 三个选项 (Options)

### 选项 A · **维持当前 `UNIQUE (env, sender, recipient)`**(推荐)

**定义**:保留全表唯一约束。被拒后用户必须点"重新申请"按钮显式触发。

**实施**:
- DB: **不变** (`migrations/0003_create_friend_requests_and_friendships.sql` 维持原样)
- im-core RelationshipService::send_friend_request:
  - 检测到 `state='rejected'` / `state='expired'` 时,**先 DELETE 旧记录** → 再 INSERT 新 `state='pending'`(在同一 tx 内)
  - 业务层显式区分"重发"和"重复发":重复发返回 409 `FRIEND_REQUEST_EXISTS`(语义:已有 pending);重发返回 201 新 `id`
- aux-03 错误码:`FRIEND_REQUEST_EXISTS`(已存在 pending)维持原义
- API 行为:`POST /v1/friends/requests {recipient_id}` 重复发同一 recipient 返回 409,被拒后重发返回 201

**优点**:
- 零迁移成本,DB 简单,索引利用率高
- 与 Telegram / Slack / Discord 主流 IM 行为一致
- 防止"用户对 A 点 5 次申请"骚扰

**缺点**:
- 不支持"被拒后立即重发"的轻量交互(用户体验差,需要先看历史才能重发)

**适用场景**:B2B / B2C 游戏内置 IM(本项目定位)

---

### 选项 B · **改为 partial UNIQUE `WHERE state='pending'`**

**定义**:只在 `state='pending'` 时强制唯一;rejected/expired/accepted 都可以重发。

**迁移**:
- 新增 `migrations/0007_relax_friend_requests_unique.sql`:
  ```sql
  -- +migrate Up
  ALTER TABLE friend_requests DROP CONSTRAINT friend_requests_environment_id_sender_id_recipient_id_key;
  CREATE UNIQUE INDEX uq_friend_requests_pending
      ON friend_requests (environment_id, sender_id, recipient_id)
      WHERE state = 'pending';
  -- +migrate Down
  DROP INDEX uq_friend_requests_pending;
  ALTER TABLE friend_requests ADD CONSTRAINT friend_requests_environment_id_sender_id_recipient_id_key
      UNIQUE (environment_id, sender_id, recipient_id);
  ```
- im-core RelationshipService::send_friend_request:
  - 检测到 `state='rejected'` 时直接 UPDATE `state='pending'`,**不** INSERT 新行
  - `updated_at` 自动由 trigger 刷新
  - 历史 `rejected` 记录保留(可追溯)

**优点**:
- 用户体验最自然(被拒后随时重发)
- 保留历史轨迹(可看出"被拒 N 次")
- 与 WeChat / QQ 早期行为一致

**缺点**:
- 加 1 个迁移文件(0007),B-1 需多跑 1 个 SQL
- rejected 记录无限累积(需 V1 加清理 job,例如保留 90 天)
- partial UNIQUE 索引查询计划略复杂,极端高并发需 EXPLAIN 验证

**适用场景**:C 端社交 IM,允许"被拒后立即重发"

---

### 选项 C · **重设计表结构(history 表分离)**

**定义**:`friend_requests` 只存"当前活跃请求",`friend_request_history` 存历史;`UNIQUE` 改为只对活跃表生效。

**迁移**(大幅):
- 新建 `friend_request_history` 表(同 schema,加 `closed_at`)
- 触发器:`state` 变更为非 `pending` 时自动搬到 history
- `friend_requests.UNIQUE (env, sender, recipient)` 维持(因为只剩 pending)

**优点**:
- 数据模型最清晰(active vs history 分离)
- 查询性能最稳定

**缺点**:
- 改动大,影响 C-1 / C-7 / D-3 全链路
- 触发器维护成本高
- MVP 阶段(only 41 WBS items)投入产出比**不划算**
- ImplementationSpec §16 P2-1 注:"多数 IM 产品也这样设计"已是隐性结论

**适用场景**:V2+ 数据量爆炸 + 需要完整审计时

---

## 3. 推荐 (Recommendation)

**推荐选项 A**。理由:

1. **MVP 范围匹配**:本项目定位"面向游戏嵌入式的 AI 原生实时通信平台",B 端为主,游戏内 IM 对"重发"诉求弱(参考 Telegram/Slack 行为)
2. **零迁移成本**:B-1 (SQL migration 真跑) 不会多 1 个文件,F-1 (Docker daemon) 解锁后即可一次跑通 6 个
3. **C-1 编码最简**:RelationshipService 只需 `INSERT ON CONFLICT DO NOTHING` + 检测返回值,无需 UPDATE 状态机
4. **ImplementationSpec §16 P2-1 已倾向**:原文"暂不修原因"是"多数 IM 产品也这样设计",等于是隐性建议保持
5. **后期可升级到 B**:若产品 V1 阶段用户反馈强需求,选项 B 是 1 个迁移文件(0007)就能切换,不是推翻重做

**不推荐选项 C**:MVP 阶段过度设计,触发器维护成本远超收益。

**不推荐选项 B**(现阶段):除非 PM 拍板"必须支持被拒后自动重发",否则 B 引入的清理 job 责任是 V1+ 负担。

## 4. 拍板选项 (PM Decision)

> Ulysses 请勾选 (A / B / C) 或在 commit 写"决定 X + 理由":

- [ ] 选项 A — 维持全表 UNIQUE,被拒后必须显式"重新申请"按钮
- [ ] 选项 B — 改为 partial UNIQUE `WHERE state='pending'`,被拒后可直接重发
- [ ] 选项 C — 重设计为 active + history 双表(不推荐,V2+ 适用)

**回填位置**:132-wbs.md §5.2 B-3 状态从 `Todo` → `Done` / `Blocked`,记录 actual token + 拍板日期。

## 5. 实施约束(以选项 A 为例的预估)

| 步骤 | 文件 | token 估算 |
|---|---|---|
| 1. PM 拍板 | (本文件 §4) | 1K(一次勾选) |
| 2. 更新 132-wbs.md 状态 | `docs/132-wbs.md` | 1K |
| 3. RelationshipService::send_friend_request 编码 (C-1) | `crates/im-core/src/relationship/service.rs` | 8K-15K(已在 C-1 600K-1.2M token 预算内) |
| 4. 集成测:重复发 / 重发两条路径 | `crates/im-core/tests/relationship_test.rs` | 3K-5K(已在 E-1 400K-800K 内) |

**总增量**:**0 token** (含在 C-1 + E-1 已批准预算内)。

## 6. 已知缺口 (Known Gaps)

- 本文件**不**替 PM 决策,3 选项 + 推荐给 PM 拍板
- 选项 B 的迁移文件 `migrations/0007_relax_friend_requests_unique.sql` 尚未创建(只有选项 A 拍板才需要)
- 选项 C 需要同步改 aux-02 §F.6-7(数据字典)+ ImplementationSpec §3.3(gRPC `SendFriendRequest` 注释)+ aux-13 §3 端点清单,影响 3 份文档,**强烈不推荐 MVP 阶段做**

## 7. 关联文档 (References)

- 上游: `docs/ImplementationSpec.md` v1.0.3 §3.1.4 好友端点 + §16 P2-1
- 上游: `migrations/0003_create_friend_requests_and_friendships.sql` 第 7-10 行 2026-08-23 自审注
- 上游: `docs/132-wbs.md` v1.0.0 §5.2 B-3 行
- 关联: `docs/aux-03-error-code-registry.md` `FRIEND_REQUEST_EXISTS` 错误码语义
- 关联: `docs/DetailedDesign.md` §5 26 REST 端点中 4 个好友端点

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 初版:3 选项 (维持 / partial UNIQUE / 重设计) + 推荐 A + 拍板栏 + token 估算 0 增量 |
