---
doc_id: H-3
title_zh: 第一个客户 PoC 范围 + 验收标准（选项清单）
status: Draft for PM Decision
owner: PM (Ulysses) / 调研: 架构师 (Mavis 接手 agent per DEC-008)
date: 2026-09-01 JST
wbs_ref: 132-wbs.md v1.0.0 §5.8 (H-3)
upstream: Project-Status.md §1.1, SRS §47 PoC Plan, BasicDesign §6/§7
---

# H-3. 第一个客户 PoC 范围 + 验收标准（选项清单）

> **本文档性质**：Mavis 调研输出，**3-5 个 PoC 候选 + 范围/验收**。**最终拍板由 PM (Ulysses) 用 ask_user 选项决定**，Mavis 不代签。
>
> **数据基线**：Project-Status.md §1.1 (MVP 范围) + SRS §47 PoC Plan (POC-01~10) + BasicDesign §6/§7 (Token Exchange + REST API 草案) + WBS C-1..C-12 token 估算
>
> **强约束**：
> - 1 人公司 Mavis 上限 1M tokens/周（per RGS-TS-001 §6.2）
> - PoC 必须"真演示"（两终端实时收发）才能算客户验证
> - F-1 Docker daemon 仍 Blocked，PoC 不能依赖 K3s 完整链路（用本地 `docker run postgres` 绕过）

---

## 1. 候选 PoC 总览

| 候选 | 主题 | 周期 | token 估算 | 风险 | 演示价值 |
|---|---|---|---|---|---|
| **PoC-01** | 单会话两终端 DM（最小演示） | 1 周 | 2.95M | 低 | 验证 §1.1 MVP 全部 4 条 |
| **PoC-02** | 3 用户群聊 + 离线增量同步 | 2 周 | 4M | 中 | 验证 Group + cursor |
| **PoC-03** | Token Exchange + Unity SDK 集成 | 3 周 | 5M | 中-高 | 验证 G2 (低成本嵌入) |
| **PoC-04** | 100K WebSocket 长连接压测 | 4 周 | 6M | 中 | SRS POC-01，非客户演示 |
| **PoC-05** | 多租户 + 客户 A 全功能 | 6 周 | 9M | 高 | 含 G-1/2 媒体，依赖 MVP 完成 |

---

## 2. PoC 候选详细规格

### PoC-01: 单会话两终端 DM（最小演示）⭐ 推荐

#### 范围
- **环境**：1 environment（dev）、1 tenant、2 users（1 owner + 1 guest）
- **会话**：1 DM conversation
- **功能**：
  - 两端 `POST /v1/auth/guest` 注册 + `POST /v1/auth/token` 拿 access_token
  - A 端 `POST /v1/conversations` 创建 DM
  - A 端 WS 帧 `send_message` 发送
  - B 端 WS 帧 `message_new` 实时收到
  - B 端 `POST /v1/conversations/{id}/read` 上报已读
  - 任意端 `GET /v1/conversations/{id}/messages?after_sequence=` 拉历史

#### 验收标准（必须全部通过）
- [ ] A 端登录拿到 access_token（< 200ms）
- [ ] A 端创建 DM conversation（< 500ms）
- [ ] A 端 send_message 收到 ack（含 message_id + sequence，< 100ms）
- [ ] B 端 WS 收到 `message_new` 帧（端到端 < 1s）
- [ ] B 端刷新页面后 GET 历史消息能拉到 A 发的（sequence 单调）
- [ ] B 端 read 上报后 A 端 `message_delivered` 帧到达
- [ ] A 与 B 互发各 10 条，无重复、无丢失、无乱序
- [ ] 本地 `docker run postgres:18.6` + 1 进程 `cargo run im-gateway` 跑通
- [ ] clippy + cargo test 全绿
- [ ] token 实际消耗 ≤ 3M（与 base 估算偏差 ≤ 30%）

#### WBS 任务清单
- C-1 im-core 6 个 PgRepository 实装 (600K-1.2M)
- C-2 MessageService::send_message 5 步实装 (300K-600K)
- C-3 POST /v1/auth/token (150K-300K)
- C-4 POST /v1/auth/guest (120K-240K)
- C-7 POST /v1/auth/logout (80K-160K)
- C-8 /v1/conversations CRUD (200K-400K)
- C-9 /v1/conversations/{id}/messages (250K-500K)
- C-11 WsSession + actix-ws 0.3 接入 (400K-800K)
- D-2 tracing_init::init (120K-240K)
- E-3 端到端 smoke (150K-300K)
- **合计 base-max：2.37M-4.74M（取 max = 3M）**

#### 适合
- 经营方要求"先有真演示"再投资
- 第一个 PoC 客户要求"2-3 周内看到东西"
- 内部 demo 优先（per Project-Status §2.1）

---

### PoC-02: 3 用户群聊 + 离线增量同步

#### 范围（在 PoC-01 基础上扩展）
- 3 users + 1 group conversation
- 增加 Group 端点 (`POST /v1/conversations` kind=group)
- 增加 `last_read_sequence` + `after_sequence` 增量同步
- 弱网测试（5-20% 丢包 + 300ms 延迟）

#### 验收标准
- [ ] 继承 PoC-01 全部 10 条
- [ ] A 创建 group，B/C 通过 conversation_id 加入
- [ ] A 群发消息，B/C 端 WS 实时收到（含 `read_count` 字段）
- [ ] B 端 kill 进程 30s，重启后 `after_sequence` 拉到离线消息不重不漏
- [ ] 弱网模拟（tc netem 5% 丢包 + 300ms 延迟）下消息不丢不重

#### 周期 / token
- 在 PoC-01 3M 基础上 + Group 端点 + 增量同步 + 弱网测试 ≈ **+1M**
- 合计 **4M tokens / 2 周**（每周 2M 超 Mavis 上限 → 实际 2 周偏紧，3 周现实）

#### 适合
- 客户品类是 MMO / Guild（需要 Group 聊天）
- 客户要求"弱网体验达标"

---

### PoC-03: Token Exchange + Unity SDK 集成

#### 范围（在 PoC-01 基础上扩展 + Unity）
- 完整 Server-to-Server Token Exchange 流程
- 简易游戏服务器（mock，用 actix-web 模拟）
- Unity 2022 LTS 工程 + C# SDK（5 步接入）
- 通过游戏服务器用 Server Secret 换 access_token → Unity 客户端 SDK 接 WS → 收发

#### 验收标准
- [ ] 继承 PoC-01 全部 10 条
- [ ] Mock 游戏服务器 `POST /game-server/token-exchange` 用 Server Secret 调 im-core
- [ ] Unity 2022 LTS 工程打开即编译通过（per SRS POC-03 集成 ≤ 1 人日）
- [ ] Unity 调用 SDK.Connect() 后 WS 建连成功
- [ ] Unity 场景内 GameObject 触发 SDK.Send() → 另一端实时收到
- [ ] **关键**（per GAME-ID-003 红线）：Unity 客户端源码中 grep 不到 `server_secret` 字面量

#### 周期 / token
- 在 PoC-01 3M 基础上 + C-5 refresh + C-6 link + Mock 游戏服务器 + Unity SDK 编写 ≈ **+2M**
- 合计 **5M tokens / 3 周**（每周 1.67M 超上限 → 实际 4 周现实）
- **风险**：Unity SDK 不在 WBS 中，需新增 WBS 任务（per 132-wbs.md §6 关键路径未含）

#### 适合
- 客户是 Unity 游戏开发商（SRS §44 兼容矩阵 P0）
- 验证 G2 (低成本嵌入) 假设
- 经营方要求"PoC 含真实 Unity 集成"以验证商业可行性

---

### PoC-04: 100K WebSocket 长连接压测

#### 范围
- 1 environment、100K 设备、单纯 WS 建连保活
- 不含消息收发（仅 WS 长连接建立 + 心跳）
- 对应 SRS POC-01 / POC-10

#### 验收标准
- [ ] 100K WS 长连接建立成功率 ≥ 99%
- [ ] 单 im-gateway 内存 ≤ X GB（待实测基线）
- [ ] 30 分钟保活无 OOM
- [ ] 弱网 5% 丢包下重连成功率 ≥ 99%

#### 周期 / token
- 需要先完成 PoC-01（C-1/C-3/C-11 必含）= 3M + 压测工具编写与执行 ≈ **+3M**
- 合计 **6M tokens / 4 周**

#### 关键
- **不适合作为"第一个客户 PoC"**——无业务演示价值，仅技术验证
- 应该是 PoC-01/02/03 完成后的内部技术验证（POC-01/10 per SRS §47）

---

### PoC-05: 多租户 + 客户 A 全功能

#### 范围
- 2-3 tenant + 客户 A 全功能（DM + Group + Presence + Retention + 媒体）
- 含 G-1 (Presence Valkey) + G-2 (Media S3 presign)
- 含多租户隔离测试（tenant A 数据 tenant B 不可见）

#### 验收标准
- [ ] 继承 PoC-01/02 全部
- [ ] 2-3 tenant 并行运行，数据完全隔离
- [ ] Presence 在线状态在 1s 内同步
- [ ] 媒体上传走 MinIO 预签名 URL，消息只存引用
- [ ] 客户 A retention 策略 30 天自动删除
- [ ] 审计日志覆盖所有租户操作

#### 周期 / token
- 依赖 G-1/2 实现（Block on H-6 S3 供应商决策）
- 合计 **9M tokens / 6 周**
- **风险**：依赖项过多（H-6 + G-1 + G-2 + multi-tenant）

#### 适合
- 第一个付费客户签约前的最终 PoC
- 不是"第一个 PoC"

---

## 3. 推荐

### 推荐 PoC：**PoC-01（单会话两终端 DM，1 周 演示）**

### 推荐理由

1. **1 人公司 1 周可交付**（Mavis 上限 1M/周 × 1 周 = 1M 容量，PoC-01 实际 3M 偏紧，2-3 周现实）
2. **真演示价值**（两终端实时收发 = MVP §1.1 全部 4 条 done definition）
3. **启动客户对话的最快路径**（per Project-Status §2.1 "MVP 必先内部 demo 再对外"）
4. **不依赖 F-1 daemon 修复**（本地 `docker run postgres:18.6` 即可绕过）
5. **覆盖 SRS §10 IM-FR-001~005**：Core 独立 + 唯一 Message ID + 单调 Sequence + 幂等 + 离线同步
6. **WBS 任务链全部在关键路径**（H-1 截止日内必交付）

### 备选

- **若客户是 Unity 游戏**：PoC-03（含 Unity SDK 集成，4 周）
- **若客户是 MMO/Guild 类**：PoC-02（3 用户群聊，2-3 周）
- **不建议**先做 PoC-04（压测无业务演示价值，per SRS §47 是内部 POC-01/10 而非客户 PoC）

### PoC 启动条件（依赖）

- H-1 截止日已决策（per 132-wbs.md §6 关键路径前置）
- F-1 不需解锁（本地 docker run 绕过）
- 客户名 + 客户承诺（"我方提供 Server-to-Server 模拟"或"我方做 2 终端人工测试"）由 Ulysses 确认

---

## 4. 已知缺口

1. **客户名 / 客户场景未明**：H-3 决策时若无具体客户，PoC 仅能走"内部 demo"形式
2. **Unity SDK 不在 WBS**：选 PoC-03 需新增 WBS 任务（per 132-wbs.md §6 当前未含 GAME SDK）
3. **F-1 daemon 修复时间不确定**：若 F-1 修复耗时 > 1 周，K3s 端到端验证受阻（PoC-01 可用本地 docker run 绕过，但失去 K3s 路径验证）
4. **token 估算偏差**：base/max 估算是经验值，实际可能 ±30%，需 actual 填入与复盘（per 132-wbs.md §5.0 跟踪规则）
5. **IM-FR-005 离线同步**在 PoC-01 验证弱（PoC-01 离线 = 杀进程 30s 后重启，弱于 PoC-02 的弱网模拟）
6. **客户验收人**未明：PoC 验收是内部还是客户？需 Ulysses 与客户定义"Done Definition"
7. **Mavis 周上限 1M 实际产能**：经验值，实际可能 0.7M/周（多轮决策对话拉低），PoC-01 3M 可能需 4-5 周实际

---

## 5. 修订历史

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 初版：5 个 PoC 候选 + 推荐 PoC-01（1 周 DM 演示） + 验收标准 10 条 |
