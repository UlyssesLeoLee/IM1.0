---
doc_id: aux-11
title_ja: 重要業務シナリオ時系列図集
title_zh: 关键场景时序图集
phase: 04-detailed-design-aux
owners: Architect
status: Draft
version: 1.0.0
related_activities: 24 架构, 46 API 详细, 84 故障注入
---

# aux-11. 重要業務シナリオ時系列図集 / 关键场景时序图集

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助)
> 责任方: Architect

## 1. 目的 (Purpose)

用 mermaid 时序图固化关键业务场景,作为跨团队沟通和代码评审的参照。

## 2. 适用范围 (Scope)

核心业务 + 关键故障 / 异常路径。

## 3. 责任方 (Owners)

Architect

## 4. 前置依赖 (Prerequisites / Inputs)

- 架构图(24)
- 46 API 详细
- 47 DB 详细

## 5. 输出 / 模板正文 (Body)

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


## 6. 验收标准 (Acceptance Criteria)

每个核心业务场景有对应时序图;时序图与代码实现一致;新场景加入时需评审。

## 7. 关联文档 (References)

- 关联工程活动: 24 架构, 46 API 详细, 84 故障注入
- 上游 Workflow: `docs/Workflow.md` Phase 4

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
