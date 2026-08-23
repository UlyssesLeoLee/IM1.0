---
doc_id: aux-05
title_ja: CRC カード
title_zh: CRC 卡(Class-Responsibility-Collaborator)
phase: 04-detailed-design-aux
owners: 開発者
status: Draft
version: 1.0.0
related_activities: 44 类设计, 43 模块设计, 45 逻辑设计
---

# aux-05. CRC カード / CRC 卡(Class-Responsibility-Collaborator)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助)
> 责任方: 開発者

## 1. 目的 (Purpose)

为每个核心类提供一张简明职责卡,聚焦做什么 / 不做什么 / 跟谁协作,作为代码评审的快速参照。

## 2. 适用范围 (Scope)

所有 public 类(尤其领域模型 / 服务 / 仓储)。

## 3. 责任方 (Owners)

開発者

## 4. 前置依赖 (Prerequisites / Inputs)

- 44 类设计
- 模块设计(43)

## 5. 输出 / 模板正文 (Body)

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


## 6. 验收标准 (Acceptance Criteria)

每个 public 类有 CRC 卡;不在职责范围内的事项写清楚;每张卡可 5 分钟内读完。

## 7. 关联文档 (References)

- 关联工程活动: 44 类设计, 43 模块设计, 45 逻辑设计
- 上游 Workflow: `docs/Workflow.md` Phase 4

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
