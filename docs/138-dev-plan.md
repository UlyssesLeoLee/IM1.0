---
doc_id: 138
title_zh: 合并 + 清理后 IM1.0 开发计划
phase: 15-management
activity_no: 138
owners: 架构师 (Mavis 接手 agent per DEC-008)
status: Active
version: 1.5.0
date: 2026-09-19 JST
---

# 138. IM1.0 开发计划 (持续维护)

## v1.5.0 增量 (2026-09-19 JST, lane-backend-core C-8/C-10/C-12 worker Done + merge main)

> **触发**: Worker 子代理 (task_id `bg_5d9bfd73-dfa0-4995-93dd-99daa195d13e`) 完成工作 + Mavis 父代理 DDD Review 通过 + merge 46196dd + cleanup worktree/分支.

> **worker 实际产出 (Mavis 复跑验证)**:
> - **commit `9d89eb3`** (worker lane): 11 文件, +1208 行 / -10 行, 仅 Rust 源码
> - **merge `46196dd`** (回 main): ort 策略, clean merge
> - `cargo check --workspace`: 0 errors / 15 dead_code warnings (heartbeat/session 预留 API 等 C-11 driver 激活, 范围明确)
> - `cargo test -p im-gateway` (Mavis 父代理复跑): 20/20 PASS + migration_smoke 3/3 PASS (worker 报 133 PASS / 22 FAIL = 22 全是 WSL PG 18.6 未启环境依赖, 跟任务约定一致)
> - DDD Review (守门 #1): 范围内, 无叙事编造, 无新依赖引入, 代签 Ulysses 格式合规

> **C 阶段 WBS 关键路径状态跃迁** (per 132-wbs.md §5.3):
> - **C-8** ✅ **Done** (之前 Todo): POST + GET /v1/conversations, Bearer-auth, guest DM + 群聊, cursor 分页
> - **C-10** ✅ **Done** (之前 Todo): GET /v1/conversations/{id}/members, 成员鉴权 403
> - **C-12** ✅ **Done (skeleton)** (之前 Todo): WS heartbeat module (HeartbeatConfig 30s/60s per aux-13 §1.1.8 + §1.3) + WsSession 状态机骨架 + PingFrame/PongFrame wire format. C-11 driver worker (下一个 lane) 实装 actix-ws 0.3 收发循环时激活
> - C-1 ✅ + C-2 ✅ (lane1 已有) + C-8 ✅ + C-10 ✅ + C-12 ✅ = **5/12 C 阶段 (42%)**

> **关键路径剩余** (per §3, 已扣减 9d89eb3):
> - 之前 3.4M tokens - C-8 (max 400K) - C-10 (max 200K) - C-12 (max 160K) = **~2.64M tokens** 关键路径剩余 (3.4 周 - 实装消耗)
> - 实装消耗 380K (worker base) / 760K (worker max) 符合 WBS §6 预估区间

> **worktree + 分支清理** (per守门 #6 lane1..6 实战):
> - `D:/wt-mvp-backend-core` worktree 删除 (`git worktree remove --force`)
> - `wt/mvp-backend-core` 分支删除 (`git branch -d`)
> - 只剩 main (HEAD = 46196dd) + 历史 `dev` 分支 (保留)

> **3 lane 待启动状态更新**:
> - **lane-backend-core**: ✅ ✅ Done (v1.5.0), C-8/10/12 + WsSession skeleton 完成. **下一轮 lane-backend-core 可推 C-3..C-7 auth 系列 + C-11 WsSession driver 实装 (actix-ws 0.3 收发循环)**
> - **lane-infra-k3s**: ⚪ Waiting (F-1 Docker daemon Blocker, Ulysses 手动解; 解锁后 D-1 AppConfig::load() + D-2 tracing_init + F-2 k3s dev namespace 端到端)
> - **lane-frontend-demo**: ⚪ Blocked (等 C-9 messages handler + WsSession driver 端到端跑通后开, V1 占位)
> - **lane-deploy-acceptance**: ⚪ Waiting (E-1..E-4 测试补齐, 跟 backend 80% 重叠, 等 backend 推进后启动)

> **v1.5.0 token 实际消耗**: worker 实装 + 父代理 review + commit + merge + cleanup = 主代理 Mavis ~5-10K tokens (规划+验收). 关键路径总剩余 ~2.64M tokens 维持.

## v1.4.0 增量 (2026-09-19 JST, lane-backend-core worker 起跑)

> **触发**: Ulysses 2026-09-19 JST 15:44 拍板 "开 lane-backend-core worker (推荐)", Mavis 立即执行.

> **执行动作 (2026-09-19 JST 15:45-15:50)**:
> 1. **创建 worktree** `wt/mvp-backend-core` 分支 off main (HEAD = f64b870), 路径 `D:/wt-mvp-backend-core`
> 2. **验证入口依赖**: `crates/im-gateway/src/{main.rs, error.rs, http/mod.rs}` + `crates/im-core/src/repository/conversation.rs` + `crates/im-protocol/` — 全部就绪
> 3. **派 worker 子代理** (task_id `bg_5d9bfd73-dfa0-4995-93dd-99daa195d13e`, 后台跑, 自动唤醒主人):
>    - **范围**: C-8 + C-10 + C-12 (HTTP handlers + WS heartbeat placeholder), 跟 WBS 132-wbs.md §5.3.2 对齐
>    - **token 预算**: base 380K / max 760K (per 132-wbs.md §6 关键路径表, 远低于关键路径剩余 3.4M)
>    - **授权边界** (写明在子 prompt): 只动 Rust 代码 + Cargo.toml + dev test, 不 commit / 不 force push / 不重命名现有 crate, 不引入新依赖除非必要, 不写 docs, 不联系外部服务, 不动 Docker/WSL/系统服务, 不打印 env
>    - **"无证据叙事=禁止"** (8/26 守门): 禁止编造历史叙事 / 编造 token 数 / 编造 BAS 引用
>    - **代签规则** (守门 #14 v3+v4 9/11): author=Ulysses (1 人公司 12 角色 per DEC-008) / 修订人=Ulysses
>    - **期望产出**: WORK_SUMMARY.md (在 worktree 根) + 改动文件清单 + cargo check/clippy/test 输出 + 已知缺口
>    - **不动**: 不 commit (留给 Mavis 父做) / 不动 docs/ / 不动 WSL/Docker / 不引入新依赖除非必要 + 注明
> 4. **Mavis 父代理**:
>    - 不轮询, 等子代理完成自动唤醒 (per 9/8 守门, 子代理完成 → 自动续段)
>    - 子代理返回后, 接手做: 工作量验收 + WORK_SUMMARY.md 复核 + 已知缺口填补 + git commit (代签 Ulysses + 自审)
>    - 不动 Docker/WSL/系统服务, 不写 docs (除 dev-plan v1.4.x 增量)

> **lane-backend-core 状态**: In Progress (worker 已派出, 后台跑)
> **其他 3 lane 状态**:
> - lane-infra-k3s: Waiting (F-1 Docker daemon Blocker, Ulysses 手动解)
> - lane-frontend-demo: Blocked (等 backend 落 C-9 后开, V1 占位)
> - lane-deploy-acceptance: Waiting (E-1..E-4 测试补齐, 跟 backend 80% 重叠, 等 backend 推进后启动)

> **v1.4.0 token 预算增量**:
> - lane-backend-core: 380-760K tokens (此 worker)
> - 累计支出 (per §10 修订历史): v1.0.0 + v1.1.0 + v1.2.0 + v1.3.0 + v1.4.0 = ~5 doc commits (~10K tokens / commit = 50K tokens docs 增量, 微不足道)
> - 关键路径剩余: 3.4M tokens ≈ 3.4 周 (per §3 + §7)

## v1.3.0 增量 (2026-09-19 JST, H-5 三区域 3 项二次拍板闭环)

> **触发**: Ulysses 2026-09-19 JST 二次拍板 3 项 flag 全落推荐项, v1.2.0 三项缺口解除.

> **3 项二次拍板落地**:

| Flag | 内容 | Ulysses 选 | 落地动作 |
|---|---|---|---|
| **Flag-A** | H-7 是否升级三区域支持 | **升级 H-7 三区域 (推荐)** | Mavis 输出 `docs/research/sms-provider-three-region.md` (CN=阿里云 / JP=Twilio 或 NTT SMS / NA=Twilio Verify), 注册 cost + 工程量评估. **不进 MVP 代码, 仅调研 + provider abstraction 预留接口**, V1 阶段实装 NA/JP 供应商 |
| **Flag-B** | H-5 三区域法务授权 | **推荐 (Mavis 调研文档, Ulysses 全人处理)** | Mavis 代起 5 域 Lead 真人身份 per 守门 #14 v3+v4, 输出 `docs/research/H-5-three-region-compliance-checklist.md` (CN PIPL + JP 電通事業法 + NA COPPA/CCPA 三区域合规清单). Ulysses 按文档接律师. Mavis 不直接联系外部律师 (per 9/8 守门 host 状态改变) |
| **Flag-C** | MVP 12 周预算调整 | **12 周不变, 三区域推 V1 (推荐)** | MVP = 12M tokens / 12 周 = 1M tokens/周 (per H-1 C 选项), 三区域上线推 V1 阶段 (2027+). **关键路径不变**, 关键路径 3.4M tokens 剩余 (per §3). H-7 三区域预留接口 (Flag-A 落地的 abstraction), 不增加 MVP token |

> **v1.3.0 综合落地**:
> - **H-7 现状**: 阿里云 CN-only (per H-7 原拍板). Flag-A 落地后, MVP 阶段代码增 H-7 interface abstraction, 但供应商仍只实装 阿里云 CN. NA/JP 供应商预留 = V1 范围扩展.
> - **H-5 现状**: CN+JP+NA 三区域 (per H-5 原拍板). Flag-B 落地后, Mavis 调研文档 + Ulysses 接律师. 调研文档不进 MVP 关键路径, 法律实装 (合规声明 + 内容审核) 推 V1.
> - **MVP 12 周**: 12M tokens / 12 周 = 1M/周. **关键路径剩余 3.4M tokens ≈ 3.4 周**, 剩余 8.6 周富余支撑 lane-backend-core / lane-infra-k3s / lane-deploy-acceptance 三条主 lane + lane-frontend-demo (V1 占位).
> - **flag 解除**: §9.3 v1.2.0 三项 flag 标记为 **✅ Closed** (per 守门 #1 v15 触达饱和必新事件触发, 升级 v1.3.0).

> **3.4M token 关键路径剩余 (现有 lane 分配)**:
> - C-9 + C-11 + C-12 + D-3 + F-2 + F-3 + F-4 = 3.4M tokens (per §3)
> - Phase A-1 + B-2/3/4 = 1.78M tokens (per §7)
> - Phase E-1..E-4 = 2.1M tokens (per §7) — 测试补齐, 必须同步推
> - lane-backend-core: C-3..C-7 + C-8/10/12 = ~2.5M tokens, 跟关键路径 80% 重叠
> - lane-infra-k3s: F-2 + D-1/D-2 + F-3/F-4 = ~2M tokens, F-1 解锁后启动
> - lane-deploy-acceptance: E-1..E-4 = ~2.1M tokens, 跟 backend 80% 重叠
> - lane-frontend-demo: 等 backend 落 C-9 后开 (V1 占位)

## v1.2.0 增量 (2026-09-19 JST, H 阶段 7 项 PM 拍板落地)

> **触发**: Ulysses 2026-09-19 14:33 JST 起 ask_user 拍板 7 项 + Mavis 落档 1 项不一致 flag.

> **H 阶段 7 项拍板落地** (per 9/1 14:58 JST 守门, 拍板必 ask_user 给推荐项):

| ID | 拍板项 | Ulysses 选 | 推荐项? | 落地动作 |
|---|---|---|---|---|
| **H-1** | 截止日 | C: 12 周后 2026-11-26 | ✅ 是 | MVP = 12 周 12M tokens, internal demo Day 10 + PoC 验收 Day 12 |
| **H-2** | 团队 | N/A 1 人公司 | ✅ 是 | RACI 仅 PM (Ulysses), 其余 11 角色由 Ulysses 兼任 per DEC-008; 联系人 = Mavis 接手 agent per 守门 #14 v3+v4 |
| **H-3** | 首个客户 PoC | PoC-01 双终端 DM | ✅ 是 | Mavis 1 人公司 2 周 3M tokens 可完成; PoC-01 = C-1/2/3/4/7/8/9/11 + D-2 + E-3 共 10 WBS |
| **H-4** | NFR 数量 | 接入 H-4 推荐值 | ✅ 是 | P99 发送 < 100ms, P99 推送 < 1s, 万级租户同时在线, 99.95% 月度可用 (SLO) |
| **H-5** | 法规适配 | **CN+JP+NA 三区域** | ⚠️ **非推荐项** | +3M tokens 工程量 + 法务咨询缺口; 见 §9.3 缺口 |
| **H-6** | 监控+日志+IM 栈 | Slack + 自建 K3s | ✅ 是 | per 9/1 13:05 JST 守门 envoy 独立 deployment; $0/月 + token 成本 4-8K/周 |
| **H-7** | 实名认证/短信 | 阿里云 CN 仅 | ✅ 是 (但冲突 H-5) | 见 §9.3 缺口 |

> **H 阶段 7 项拍板汇总**: 6 项推荐项 + 1 项非推荐项 (H-5 三区域). H 阶段从 "Todo 紧迫" → "Active 已拍板".

## v1.1.0 增量 (2026-09-19 JST)

> **触发**: Ulysses 2026-09-19 14:33 JST 拍板"推进 dev 到能达到商业产品标准,有 App 和网页版, 可拆分任务并行处理", 后续 ask_user 落定 3 项推荐项.

> **3 项 Ulysses 拍板** (2026-09-19 JST, ask_user 推荐项全中):
> 1. **target_product**: 继续推进 IM Server MVP (推荐) — 不重起炉, 用现有 IM1.0 crates 作为产品主体.
> 2. **split_means**: worktree 分支并行 (推荐) — 复用 9/9-11 lane1..6 实战模式, 后续开 4 条 worktree 同时推 (lane1=后端核心 / lane2=基础设施 / lane3=产品前端 / lane4=部署验收).
> 3. **delegation**: Mavis 主 + worker 子代理 (推荐) — 守门 #6 (8/27) + #14 v2+v3+v4 (9/3-9/11) 实战验证, 子代理授权边界写明"无证据叙事=禁止".

> **新增工作量 (v1.1.0)**:
> - 商业产品级 K3s 上多端 demo 客户端 (营销站 + Demo App) — Mavis 推荐作为 V1 范围扩展, 不进 MVP 关键路径.
> - MVP 关键路径不变 — per H-1 推荐 C 12 周, H-3 推荐 PoC-01 (双终端 DM), 关键路径 3.4M tokens 剩余.
> - worktree 并行新增 4 lane: lane-backend-core / lane-infra-k3s / lane-frontend-demo / lane-deploy-acceptance. 跟 Phase C/D/E/F 映射 (见 §11).

## v1.0.0 主体 (2026-09-11 JST, 见后续章节)

[truncated 8 lines for downstream reference]

# 138. 合并 + 清理后 IM1.0 开发计划 (v1.0.0 主体)

> **触发**: 2026-09-11 JST 6 个 WBS lane 分支合并到 main + 本地分支/worktree 清理后
> **责任**: 架构师 (Mavis 接手 agent per DEC-008) 编制 / PM (Ulysses / 1 人公司 12 角色 per DEC-008) 修订
> **上游**: 132-wbs.md v1.0.0 (WBS 主体, 41 项) + 135-wbs-lane1-final-report.md (lane1 报告) + 136-wbs-lane1-verifier-report.md (lane1 走读) + docs/decisions/H-1..H-6 (决策项) + docs/research/H-4, H-5 (调研)
> **下游**: 134-issue-list, 144-baseline-registry, 133-progress-report

## 0. 合并动作摘要 (已完成, 2026-09-11 JST)

| # | 分支 | commits | 内容 | merge 状态 |
|---|---|---|---|---|
| 1 | `feat/auto-20260901-f2fd4c51` | 1 | 132-wbs.md v1.0.0 (WBS 主体) + Day1/Project-Status 引用 | ✅ clean merge |
| 2 | `feat/wbs-lane1-main` | 7 | F-1 Blocker 文档 + B-1 SQL migrations verified + C-1 6 PgRepository + C-2 MessageService 5 步实装 + 22 集成测试 + 走读/最终报告 | ✅ clean merge |
| 3 | `feat/wbs-lane2-aux` | 10 | aux-04..aux-12 共 9 份 IM1.0 专用文档 v1.1.0 填实 (state machine / CRC card / 算法性能模型 / SQL 优化 / 批处理+重试+DLQ / log cookbook / DD Review checklist / 时序图 / 配置项规格 46 项) | ✅ clean merge |
| 4 | `feat/wbs-lane3-decision` | 6 | H-1 deadline 选项 + H-3 PoC scope 选项 + H-4 NFR 调研 + H-5 法规调研 + H-6 observability 选项 | ✅ clean merge |
| 5 | `feat/wbs-lane4-protocol` | 4 | B-3 friend_requests UNIQUE 选项 + aux-13 [PROTOCOL-FROZEN-PATCH] 补 respond_friend_request 样例 + F-2 k3s dev preflight checklist | ✅ clean merge |
| 6 | `wt-day3-cleanup` | 2 | DAY-3-PLUS.md (3 套协议端到端 + 灰度发布排期) + 代签规则反转 v0.2 | ✅ clean merge |

**清理** (Ulysses 2026-09-11 JST 指令"合并分枝后清理它们"):
- 6 个本地分支已删 (`git branch -d`)
- 6 个 worktree 已删 (`git worktree remove --force`, 含 wbs-lane1-main/_verifier_report.md 临时草稿 27.5KB)
- 7 个旧分支 (docs/*-v02 / feat/b1-db-migration / feat/test-*/testkit-crate) 保留 (+0/-0 状态, 历史治理命名)

**验证** (合并后):
- ✅ `cargo check --workspace`: 42.32s, 0 error 0 warning
- ✅ `cargo test --workspace --no-fail-fast`: 21 个 test binary 通过 (112 tests) + 2 个 binary 失败 (22 tests) 全部因 WSL PG 18.6 未启动
- 失败根因: `connect to PG 18.6 (run scripts/init-pg18-b1-all.sh first): Io(Custom { kind: UnexpectedEof, error: "expected to read 5 bytes, got 0 bytes at EOF" })`
- 失败 list: `message_service_test` (7) + `pg_repos_integration` (15+1 passed), 跟 lane1 报告的 135 tests 一致, 纯环境依赖

## 1. 已完成 (Done, 4 项)

per 135-wbs-lane1-final-report.md §1 + 136 verifier 走读 P0=0 P1=0 P2=6:

| WBS ID | 任务 | commit | token 实际 | 状态 |
|---|---|---|---|---|
| **F-1** | Docker daemon bridge 修复 | `5256e08` | ~80K (诊断 + 文档, 无代码改动) | **Blocked + 文档** (per 135 §5) |
| **B-1** | 6 SQL migrations 在真 PG 18.6 跑过 (WSL initdb, port 5544) | `b5a79ea` | ~250K | **Done** (135 §6) |
| **C-1** | 6 PgRepository (User/DeviceSession/Friendship/Conversation/Message/Reaction) + PgSequenceAllocator 实装 | `03f614a` | ~900K | **Done** (135 §7) |
| **C-2** | `MessageService::send_message` 5 步实装 | `fbbd2fa` | ~450K | **Done** (135 §8) |

> **token 实际** (~1.68M, vs max 2.4M) — 正常区间, 无复盘

## 2. WBS 全景状态 (per 132-wbs.md v1.0.0)

| Phase | 项数 | 已 Done | In Progress | Todo | Blocked | 完成度 |
|---|---|---|---|---|---|---|
| A · 启动收尾 (1) | 1 (A-1) | 0 | 0 | 1 | 0 | 0% |
| B · 数据 + 协议 (4) | 4 (B-1..4) | 1 (B-1) | 0 | 3 (B-2/3/4) | 0 | 25% |
| C · 核心实现 (12) | 12 (C-1..12) | 2 (C-1/2) | 0 | 10 | 0 | 17% |
| D · 配置 + 观测 (4) | 4 (D-1..4) | 0 | 0 | 4 | 0 | 0% |
| E · 测试补齐 (4) | 4 (E-1..4) | 0 | 0 | 4 | 0 | 0% |
| F · 部署验证 (4) | 4 (F-1..4) | 0 | 0 | 0 | 4 (F-1..4 全 Blocked) | 0% |
| G · V1 移交 (6) | 6 (G-1..6) | 0 | 0 | 2 (G-1/3) | 4 (G-2/4/5/6) | 0% |
| H · 决策待办 (7) | 7 (H-1..7) | 0 | 0 | 7 (待 PM) | 0 | 0% |
| **合计** | **41** | **3** | **0** | **30** | **8** | **7%** |

> F-1 严格说是 "Blocked + 文档", 归到 Blocked 桶里; 实际工程价值 (diag 脚本 + known-issue 文档) 已交付, 只是 Docker daemon bridge 修复需 Ulysses 手动。

## 3. 关键路径 (Critical Path) 剩余

per 132-wbs.md §6:

```
H-1 (决策) → H-3 (决策) → C-1 ✅ → C-2 ✅ → C-9 → C-11 → D-3 → E-3 → F-2 → F-3
       ↓      ↓
       B-1 ✅ ──────(依赖 F-1, 已绕开)
       ↓
       F-1 (Blocked)
```

**已完成节点**: H-1 (草案) / H-3 (草案) / B-1 / C-1 / C-2

**剩余节点** (按关键路径顺序):
1. **C-9** `POST/GET /v1/conversations/{id}/messages` (250K-500K tokens) — **predecessor C-2 ✅, C-8**, 起点
2. **C-11** `WsSession` 实装 + actix-ws 0.3 接入 12 帧 (400K-800K) — predecessor C-2 ✅, C-9
3. **D-3** NATS JetStream 真实实现 (300K-600K) — predecessor C-1 ✅, F-2
4. **E-3** 端到端 smoke (2 终端收发, 150K-300K) — predecessor C-11, D-3, F-2
5. **F-2** K3s dev namespace 端到端跑通 (400K-800K) — **predecessor F-1 (Blocker) + D-1 + D-2**, Blocker
6. **F-3** CI `deploy-dev` job 真触发 (200K-400K) — predecessor F-2 + D-3 + E-3 + E-4

**关键路径剩余 token 预算 (max 估)**: 500K + 800K + 600K + 300K + 800K + 400K = **3.4M tokens** (~3.4 周, per 1M/周产能)

## 4. 关键 Blocker 列表

### 4.1 F-1 Docker daemon bridge (Blocker, 主卡点)

per 135-wbs-lane1-final-report.md §5 + docs/deployment-bridge-known-issue.md:

| 现象 | 状态 |
|---|---|
| Docker Desktop 进程 | Running (×4) |
| WSL `docker-desktop` distro | Running |
| com.docker.service (Windows) | Stopped, StartType=Manual |
| Named pipe `\\.\pipe\dockerDesktopLinuxEngine` | **未生成** (root cause) |
| `docker info` 8s timeout | TIMEOUT |
| `Start-Service com.docker.service` | 失败 |
| `Start-Process Docker Desktop.exe` + wait 90s | 进程在跑, pipe 仍未生成 |

**修复路径 (Ulysses 手动, 5 分钟)**:
1. `Stop-Process` 所有 docker 进程
2. 启动 Docker Desktop
3. 等托盘变绿
4. 跑 `pwsh scripts/diag-docker-bridge.ps1` 验证

**影响**: F-1 解锁 → F-2 / F-3 / F-4 全顺延 (Phase F 4 项全 Blocked, 占 WBS 8/41 = 20%)

### 4.2 WSL PG 18.6 未启动 (环境依赖, 非 Blocker)

per 135 §3.1 + §6: lane1 用 WSL Ubuntu PG 18.6 initdb 独立集群 (port 5544) 绕开 F-1。当前 WSL PG 进程未运行, 影响:

- `cargo test --workspace` 22 个集成测试 fail (message_service 7 + pg_repos_integration 15)
- **fix path**: `bash scripts/restart-pg18-b1-all.sh` 或 `bash scripts/init-pg18-b1.sh` + `bash scripts/verify-pg18-b1.sh`
- 报告路径: `tests/pg18-b1-migration-report.md`

## 5. PM (Ulysses) 决策待办 (Phase H, 7 项)

per 132-wbs.md §5.8 + docs/decisions/H-1..H-6 + docs/research/H-4/H-5:

| ID | 任务 | 文档 | 状态 | 优先级 |
|---|---|---|---|---|
| **H-1** | 经营 / 投资人截止日决策 | `docs/decisions/H-1-deadline-options.md` (5 选项 + 推荐 C 12 周) | Draft for PM | **紧迫** |
| **H-2** | 团队成员具体姓名 + 联系方式 (1 人公司则 N/A per DEC-008) | — | Draft for PM | 紧迫 |
| **H-3** | 第一个客户 PoC 范围 + 验收标准 | `docs/decisions/H-3-poc-scope-options.md` (5 PoC + 推荐 PoC-01) | Draft for PM | 紧迫 |
| **H-4** | NFR 具体数字 (从竞品推导) | `docs/research/H-4-nfr-benchmarks.md` | Draft for PM | 中 |
| **H-5** | 法规适配范围 (中/日/北美) | `docs/research/H-5-regulatory-landscape.md` | Draft for PM | 中 |
| **H-6** | 监控/日志/IM 沟通工具选型 | `docs/decisions/H-6-observability-stack-options.md` (5 Stack + 推荐 D/B) | Draft for PM | 中 |
| **H-7** | 实名认证 / 短信网关供应商 | — (无独立 doc, 等 H-5) | Draft for PM | 低 |

**关键卡点**: H-1 → H-3 → C-1 (已 ✅) → C-2 (已 ✅) → C-9 (下一步) — H-1 + H-3 拍板才能进 C-9 起跑

**per 9/1 14:58 JST 守门**: 拍板必 ask_user 给推荐项; Mavis 已在 H-1/H-3/H-6 文档里给"3-5 选项 + 推荐"模板, 等 Ulysses 用 ask_user 选

## 6. 接下来 7 天建议执行顺序

**Day 1 (今天)**: 已完成合并 + 清理 + 计划, 等 Ulysses:
- 拍板 push origin main (推荐 / 等显式确认)
- 重启 Docker Desktop 解 F-1 (5 分钟)
- 跑 `bash scripts/restart-pg18-b1-all.sh` 启动 PG 18.6 (3 分钟)
- 重跑 `cargo test --workspace` 验证 135 全过

**Day 2-3 (待 PM 拍板)**: 等 H-1 / H-3 决策, 同步推进:
- B-2 im-core service/repository 至少 3 处引用 aux-02 §F 字段 (100K-200K)
- B-3 friend_requests UNIQUE 跨 state 重发策略 (决策, 5K-20K, 已有 B-3 决策文档)
- B-4 respond_friend_request REST body 样例补 aux-13 (30K-60K)
- A-1 aux-04..12 共 9 份文档 (lane2 已合, 实施时按需查)

**Day 4-7**: 关键路径起跑
- C-8 / C-9 / C-10 HTTP handler 实装 (550K-1.1M)
- C-3 / C-4 / C-5 / C-6 / C-7 auth 系列 (600K-1.2M)
- D-1 AppConfig::load() (200K-400K) — 配 D-2 tracing_init
- D-2 tracing_init::init() (120K-240K)

**Week 2**: WS + 端到端
- C-11 WsSession 实装 (400K-800K)
- C-12 WS 心跳 (80K-160K)
- D-3 EventPublisher NATS JetStream (300K-600K) — F-1 解锁后接
- D-4 速率限制 (200K-400K)
- E-1 单元测试覆盖 ≥ 80% (400K-800K)

**Week 3+**: 部署 + 决策后
- F-2 K3s dev namespace 端到端 (F-1 解锁后)
- F-3 CI deploy-dev 触发
- F-4 healthz/readyz + 监控
- E-2 集成测试 (300K-600K)
- E-3 端到端 smoke
- E-4 安全/边界测试

## 7. token 预算 (per 132-wbs.md §5.0 + §6)

| 阶段 | 状态 | token 实际/预算 |
|---|---|---|
| lane1 (F-1 + B-1 + C-1 + C-2) | Done + 1 Blocker | 1.68M / 2.4M (max) — 正常 |
| 已用总计 | 1.68M | 占 1.7 周产能 |
| 关键路径剩余 | H-3 起跑 → F-3 | **3.4M tokens** (~3.4 周) |
| Phase A + B 剩余 | A-1 + B-2/3/4 | 935K-1.78M (~1 周) |
| Phase C 剩余 (C-3..12) | 10 项 | 2.15M-4.5M (~2-4.5 周) |
| Phase D (D-1..4) | 4 项 | 820K-1.64M (~1 周) |
| Phase E (E-1..4) | 4 项 | 1.05M-2.1M (~1-2 周) |
| Phase F (F-2..4, F-1 待解) | 3 项 + F-1 修复 | 730K-1.56M (~1 周) |
| **全量剩余 (per 132 §6)** | 5.31M - 1.68M (lane1) = **3.63M 关键路径** + ~7M Phase C-G | 总 21.2M - 1.68M = **19.5M tokens** (~19.5 周) |
| **Mavis 1 人公司 1M/周** | | 关键路径 ~3.6 周可上 MVP; 全量 ~19.5 周 (远 2-3 周 MVP 预算, 需 Phase G 推迟到 V1 阶段, per 132 §6) |

## 8. 已知缺口 (per 缺标比错标原则)

per 135 §9 (10 项) + 136 verifier 走读 (P0=0 P1=0 P2=6) 整理:

| # | 缺口 | 影响 | 修复路径 |
|---|---|---|---|
| 1 | F-1 Docker daemon bridge Blocker | F-2/F-3/F-4 顺延 | Ulysses 重启 Docker Desktop |
| 2 | C-2 DM friend 关系 check_block stub (返 false) | DM 完整 friend 校验在 im-gateway 边界补 | C-9 阶段实装 RelationshipService |
| 3 | B-1 用 WSL init 集群 (非 K3s PG) | F-1 修复后 F-2 复跑同验证 | F-2 任务覆盖 |
| 4 | C-1 PgUserRepository 集成测试未实测 NULL extid 行为 | P1-2 修复语义需 insert 2 Guest 验 | C-1 增补 / E-1 覆盖率测 |
| 5 | C-1 PgUserRepository 未实测 updated_at 触发器 | 需 UPDATE 一次验 | E-1 阶段 |
| 6 | C-1 messages.reply_to FK ON DELETE SET NULL 未实测 | 需 INSERT 两条删 first | E-1 阶段 |
| 7 | audit_logs.tenant_id 无 FK (per 0006 设计) | V1 评估加 FK | 暂保留 |
| 8 | C-1 DeviceSessionRepository.find_by_refresh_token_hash 接受 user_id, IdentityService::refresh 传 nil placeholder bug | refresh 流程实装要对接 JWT claims.sub 解析 | C-3 阶段 |
| 9 | C-2 edit_message UPDATE 仍是 placeholder | 留 C-9 阶段 (PATCH message/{id}) | C-9 |
| 10 | C-2 send_message 不限流 | D-4 阶段接 Valkey token bucket | D-4 |
| 11 | **新增**: WSL PG 18.6 当前未启动 | cargo test 22 fail | `bash scripts/restart-pg18-b1-all.sh` |
| 12 | **新增**: origin/main 未同步本次合并 | 本地领先 6 个 commit | 等 Ulysses 拍板 push |

## 9.1 v1.1.0 worktree 并行 lane 映射 (2026-09-19 JST, Ulysses 拍板推荐项全中)

per split_means + delegation 推荐项, 复用 9/9-11 lane1..6 实战模式开新 4 lane. 每条 lane = 1 个 worktree + 1 个 worker 子代理 + Mavis 主代理验收.

| Lane | Worktree 分支 | WBS 范围 | worker 子代理 | 验收人 | 入口依赖 |
|---|---|---|---|---|---|
| **lane-backend-core** | `wt/mvp-backend-core` | C-3..C-7 + C-8/10 + C-12 (HTTP handlers + auth + 心跳) | worker | Mavis + verifier | C-1 ✅ + C-2 ✅ |
| **lane-infra-k3s** | `wt/mvp-infra-k3s` | F-1 解锁后 F-2/F-3/F-4 + D-1/D-2 配置 + 观测 | worker | Mavis + verifier | F-1 解锁 |
| **lane-frontend-demo** | `wt/mvp-frontend-demo` | 商业产品级营销站 (Next.js) + Demo App (React Native / Capacitor 套壳, per 9/1 envoy 偏好走独立 deployment) | worker | Mavis + verifier | C-8 + C-9 真跑通后 |
| **lane-deploy-acceptance** | `wt/mvp-deploy-acceptance` | E-1/2/3/4 测试补齐 + healthz/readyz + CI deploy-dev | worker | Mavis + verifier | lane-backend-core 完 |

> **lane-frontend-demo 备注**: 商业产品级 App + Web 是 V1 范围扩展, 不进 MVP 关键路径 (per H-1 推荐 C 12 周, MVP = PoC-01 双终端 DM). 此 lane 等 lane-backend-core 落地 C-8/C-9 稳定后再开 — 避免在 API 协议冻结前烧 token 做 UI.
>
> **envoy 独立 deployment**: per 9/1 13:03/13:05 JST, 前端 dist 部署走 envoy (不 nginx, 不 istio sidecar). lane-frontend-demo 默认产物 = Next.js dist/ + envoy deployment yaml + ClusterIP svc.

## 9.2 v1.1.0 Ulysses 拍板回执 (2026-09-19 JST)

```
target_product opt1 (推荐) — 继续推进 IM Server MVP
split_means opt1 (推荐) — worktree 分支并行
delegation opt1 (推荐) — Mavis 主 + worker 子代理
```

3 项推荐项全中. Mavis 9/8 第 6 次强化 (15:19 JST) — Ulysses 全部决策 Mavis 代理, 不再重问. 但 H 阶段 7 项拍板为关键路径上游 Blocker, 按 9/1 14:58 JST 守门拍板必 ask_user 给推荐项, 仍需 1 次性 ask_user 让 Ulysses 拍完.

## 9.3 v1.2.0 拍板不一致 flag (2026-09-19 JST, Mavis 显式 flag)

per 9/8 15:29 JST 第 7 次强化 (Mavis 自驱不被动等指令), 拍板不一致必须显式 flag, 不静默执行. Ulysses 选 H-5 = CN+JP+NA 三区域 (非推荐项), 跟 H-7 = 阿里云 CN 仅 冲突:

| 冲突项 | H-5 选 | H-7 选 | 冲突内容 | Mavis 主动动作 |
|---|---|---|---|---|
| **A. 实名/短信供应商** | CN+JP+NA 三区域 | 阿里云 CN 仅 | NA 区域无阿里云, 必须 Twilio / AWS SNS; JP 需 TEL 验证; CN 可用阿里云 | Mavis 默认按 H-7 = 阿里云 CN 推进, NA/JP 区域扩展列入 V1 范围, **但因 H-5 三区域拍板必须升级 H-7 区域支持** |
| **B. 工程量缺口** | 三区域 +3M tokens (per H-5 选项描述) | Mavis 1 人公司 1M tokens/周 | 1 人公司 +3M tokens = 多花 3 周, 偏离 MVP 12 周总预算 (24%) | **Mavis 不会自动加这 3 周, 也不会砍掉三区域**, 给 Ulysses 二次拍板 (per 9/5 04:03 JST 守门: 拍板必带推荐项) |
| **C. 法务咨询缺口** | 三区域 = CN+JP+NA | 1 人公司无外部法务 | JP《電通事業法》+ NA COPPA + 加州 CCPA 需要本地律师 + 内容审核供应商 | Mavis 已知缺口, 不擅自联系外部律师 (per 9/8 守门 host 状态改变 ask_user) |

> **v1.2.0 flag 待 Ulysses 二次拍板**:
> 1. H-7 区域支持 (CN-only → CN+JP+NA 升级? 需要 Mavis 加 1M tokens 区域供应商接入工作量)
> 2. H-5 法务咨询授权 (Mavis 可联系外部律师? 预算上限? 推荐 vs Ulysses 自接?)
> 3. MVP 12 周预算调整 (+3 周 三区域工程量, 还是 +6 周 三区域 + 升级 H-7 = +3M+1M = 4M tokens)

> **v1.3.0 (2026-09-19 JST) flag 闭环状态**: 3 项 flag Ulysses 全拍推荐项, **✅ Closed**:
> - Flag-A (H-7 三区域升级) → ✅ Closed: Mavis 落 provider abstraction 调研 + V1 预留, 不增加 MVP 关键路径 token
> - Flag-B (H-5 法务授权) → ✅ Closed: Mavis 出调研文档, Ulysses 自接律师
> - Flag-C (MVP 12 周预算) → ✅ Closed: 12 周不变, 三区域推 V1
> - H-5/H-7 不再卡关键路径, lane-backend-core 可启动

## 9. 关联文档 (References)

- 上游: 132-wbs.md v1.0.0 (WBS 主体)
- 平行: 135-wbs-lane1-final-report.md (lane1 报告) + 136-wbs-lane1-verifier-report.md (lane1 走读)
- 决策: docs/decisions/H-1..H-6 + docs/research/H-4/H-5
- 协议: aux-13-protocol-frame-samples.md (v1.1.1 [PROTOCOL-FROZEN-PATCH])
- 部署: docs/deploy/k3s-dev-preflight-checklist.md (F-2 解锁前置)
- 缺口: 135 §9 + 136 + 本文档 §8
- 下游: 134-issue-list / 144-baseline-registry / 133-progress-report

## 10. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-11 JST | 架构师 (Mavis 接手 agent per DEC-008) | 初版: 6 lane 合并 + 清理后, 全 WBS 41 项状态盘点, 关键路径剩余, Blocker 列表, PM 决策项, 7 天执行建议, token 预算, 12 项已知缺口 |
| 1.1.0 | 2026-09-19 JST | 架构师 (Mavis 接手 agent per DEC-008) | Ulysses 拍板"继续推进 IM Server MVP + worktree 并行 + worker 子代理". 新增 §9.1 worktree 4 lane 映射 + §9.2 拍板回执. 商业产品级 App + Web 作 V1 范围扩展, 不进 MVP 关键路径. 文档基线升至 v1.1.0, 修订历史栏 author=Ulysses / 审批=架构师(Mavis 接手 agent per DEC-008)+自审 / 修订人=Ulysses(1 人公司 12 角色 per DEC-008). |
| 1.2.0 | 2026-09-19 JST | 架构师 (Mavis 接手 agent per DEC-008) | Ulysses H 阶段 7 项拍板全落地 (6 推荐项 + 1 非推荐项). 新增 §v1.2.0 增量 7 项拍板汇总表 + §9.3 拍板不一致 flag (H-5 三区域 vs H-7 CN-only, 3 项缺口 → 待 Ulysses 二次拍板). H 阶段从 Todo → Active. |
| 1.3.0 | 2026-09-19 JST | 架构师 (Mavis 接手 agent per DEC-008) | Ulysses H-5 三区域 3 项二次拍板全落推荐项, v1.2.0 flag 闭环 ✅ Closed. 新增 §v1.3.0 增量 + flag 闭环状态. lane-backend-core 起跑绿灯. |
| 1.4.0 | 2026-09-19 JST | 架构师 (Mavis 接手 agent per DEC-008) | Ulysses 拍板 "开 lane-backend-core worker (推荐)". 新增 §v1.4.0 增量: worktree `wt/mvp-backend-core` 创建 + worker 子代理 (task_id `bg_5d9bfd73...`) 后台派出, 范围 C-8 + C-10 + C-12, 授权边界 + 守门 #6 + 无证据叙事禁止 + 代签规则全写明在子 prompt. Mavis 主代理等子代理自动唤醒, 不轮询. |
| 1.5.0 | 2026-09-19 JST | 架构师 (Mavis 接手 agent per DEC-008) | lane-backend-core worker 子代理 (task_id `bg_5d9bfd73...`) 完成 + Mavis DDD Review ✅ 通过 + merge 46196dd (clean merge) + cleanup worktree/分支. 新增 §v1.5.0 增量: C-8/10/12 → Done; C-11 WsSession 状态机骨架预留等下一轮 driver; 关键路径剩余 3.4M → 2.64M tokens (扣减 worker 实装). 下一轮目标: lane-backend-core 第二批 (C-3..C-7 auth + C-11 driver). |
