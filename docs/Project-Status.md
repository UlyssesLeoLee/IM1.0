# IM1.0 项目当前状态 (Project Status)

> **本文档是 IM1.0 当前决策快照** —— Day 1 Kickoff 后的实时状态。
> 任何决策变更必须更新本文档,并通知团队。
>
> 关联文档:
> - `Workflow.md` —— 150 工程活动主流程
> - `Workflow-RACI.md` —— 角色责任分配
> - `Day1-Task-List.md` —— Day 1 任务清单
> - `Platform-Specifics.md` —— GitHub / K3s 平台特定配置
> - `132-wbs.md` —— WBS 工作分解结构 (Phase A-H 全量, token 单位)

---

## 0. 更新记录

| 版本 | 日期 | 修订人 | 变更 |
|---|---|---|---|
| 1.0.0 | 2026-08-20 | (Mavis 辅助) | Day 1 Kickoff 决议 |
| 1.1.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 新增 132-wbs.md 关联;决策待办 7 项 (§2.2) 仍空缺,卡 H-1 截止日 |

---

## 1. 已决策(4 项)

### 1.1 MVP 第一个 PR 范围:消息收发 (IM Core 最小闭环)

- **范围**:用户认证 + 单聊发消息 + 实时接收推送
- **排除**:群聊、文件、表情、消息撤回、跨房间、推送通知
- **完成定义**:
  - 1 个用户能登录拿到 Token
  - 1 个用户能给另 1 个用户发消息
  - 接收方能实时收到(WebSocket)
  - 消息持久化,刷新后能查历史
- **预计工期**:~2 周(2-3 人极简团队)

### 1.1.1 Day 2 GATE 补签里程碑 (2026-08-26) [PROTOCOL-FROZEN]

- **协议冻结**:WS 12 个帧 + gRPC 22 个 RPC + REST 26 个端点(7 类)三套协议已冻结(详细见 ImplementationSpec §3.2.4 + aux-13 §6 末段)。
- **clippy 阻断通过**:`cargo clippy --workspace --all-targets -- -D warnings` EXIT 0(占位模块加 `#![allow(dead_code, unused_imports, unused_variables)]`,V1 实装时移除)。
- **测试通过**:`cargo test --workspace` 27 tests passed / 0 failed(im_common 8 / im_core 12 / im_protocol 7)。
- **遗留工程债**:
  - im-core 各 service / repository **未直接引用** aux-02 §F 字段定义(只有 `im-common/src/ids.rs:3` 1 处 + 6 个 migration 注释引用;ImSpec §12.3 要求"im-core 各 service / repository 至少 3 处引用 aux-02",未达成)。
  - 6 份 SQL migration 未在真 PG 实例上跑过(2026-08-26 沙箱 Docker Desktop 启了但 Windows↔WSL2 daemon bridge 未就绪,`docker ps` 2 分钟超时;`postgres:18.6` image 锁 tag 已 commit,K3s dev 部署 / 桌面端 daemon bridge 就绪后立即可验证)。

### 1.2 第一个产品线:IM Core (消息为主)

- **优先级**:P0
- **范围**:im-gateway + im-router + im-store + 协议栈
- **下游依赖**:
  - Voice Subsystem(等 IM Core 跑通后再启动)
  - Game SDK(等 IM Core + Auth 稳定后启动)
- **不做并行**:Voice / SDK 等 IM Core MVP 上线后启动

### 1.3 仓库 + CI 平台:GitHub + GitHub Actions

- **仓库位置**:`github.com/{org}/im1.0`(待确认组织名)
- **CI**:GitHub Actions(详见 `Platform-Specifics.md`)
- **镜像仓库**:GHCR(`ghcr.io/{org}/im1.0-*`)
- **包管理**:
  - Rust:crates.io + 私有 registry 待定
  - 私有 NuGet / npm:暂不启用
- **文档**:Markdown in repo(`/docs`),不引外部 wiki
- **Issue / Project**:GitHub Issues + GitHub Projects

### 1.4 团队规模:2-3 人 (极简)

- **角色合并**:
  - Tech Lead = 主开发者(占 ~80% 时间)
  - PM = Tech Lead 兼
  - SRE = 1 名成员兼(无独立 SRE)
  - QA = 所有人兼,无独立 QA
- **流程简化**(对比 `Workflow.md` 完整 16 Phase):
  - 跳过完整 BD Review / DD Review(改为"自评 + Tech Lead 拍板")
  - 跳过 UAT(改为内部 smoke)
  - UT 覆盖率:Line ≥ 60%(完整流程是 80%,极简放宽)
  - ITa / ITb 合并(没人力分两阶段)
  - 12 个 GATE 简化为 4 个:**PR 合入 / 单测通过 / Smoke 通过 / 部署成功**
- **不简化**:
  - CI 阻断(SAST Critical = 0、CR 1 人 review + 1 人 approve)
  - 错误码注册(aux-03)
  - 命名规范(aux-01)
  - 数据字典(aux-02)
  - 状态机(aux-04)
  - 协议冻结(aux-13)

### 1.5 技术栈选型(Day 1 Kickoff 追加)

| 类别 | 选型 | 版本策略 |
|---|---|---|
| 语言 | **Rust** | 最新 stable(major tag 跟随 6 周节奏) |
| Web 框架 | **actix-web 4.x** | `"4"`(拿 4.x 最新) |
| WebSocket | **actix-ws 0.3** | MVP 用;后续可换 actix-web-actors |
| 异步运行时 | tokio | actix 内置 |
| DB | **PostgreSQL 18.6** | 2026-08 锁 patch level(per Ulysses 2026-08-26 指令) |
| DB 驱动 | **sqlx 0.9** | async-friendly;带 migration 工具(Day 1 GATE 补签 0.8→0.9) |
| 前端(MVP 后) | Next.js | 已有(详见 README) |
| 部署 | K3s + Docker | 单节点 dev → 后续多节点 prod |
| 镜像 | GHCR | 私有 registry |
| CI/CD | GitHub Actions | 详见 `Platform-Specifics.md` §3 |

**不用**:
- ❌ axum / tower / hyper(统一 actix)
- ❌ diesel(用 sqlx)
- ❌ Redis(MVP 阶段不需要)
- ❌ LiveKit / Voice(留给 Voice 阶段)

**版本跟随**:
- Rust:`rust:1-slim` 自动拉最新(stable rolling tag,2026-08 系统为 1.98.0);CI 用 `dtolnay/rust-toolchain@stable`
- PostgreSQL:`postgres:18.6` 锁 patch level(2026-08-26 per Ulysses 指令);升级到 18.7 / 19.x 时先在 staging 跑一周
- 详细 spec 见 `Platform-Specifics.md` §5

---

## 2. 待决策(1 项,Day 1 启动前必须答)

### 2.1 关键里程碑日期

| 节点 | 当前 | 待填 |
|---|---|---|
| IM Core MVP 上线 | TBD | (YYYY-MM-DD) |
| 第一个客户 PoC | TBD | |
| 第一个付费 / GA | TBD | |
| 完整 Voice + SDK 集成 | TBD | |

**约束**:
- 经营 / 投资人有截止日 → **必须给具体日期**,不能"尽快"
- MVP 必须先内部 demo(给团队 + 经营)再对外
- 招人节奏:是否在 MVP 之前补齐 4-6 人?

> **催办**:此项目卡 Day 1 启动。请在 24h 内答复。

### 2.2 待补(不影响 Day 1 但应尽快)

- [ ] 经营 / 投资人截止日
- [ ] 团队成员具体姓名 + 联系方式
- [ ] 第一个客户的 PoC 范围 + 验收标准
- [ ] NFR 具体数字(从经营要求 / 竞品分析推导)
- [ ] 法规适配范围(中国 / 日本 / 北美)
- [ ] 监控 / 日志 / IM 沟通工具选型
- [ ] 实名认证 / 短信网关供应商

---

## 3. 简化流程与原始流程的对照

| 流程节点 | 完整流程 | 极简流程(2-3 人) | 备注 |
|---|---|---|---|
| BD Review(41) | 多人 + 客户 | Tech Lead 1 人 | 走"自评 + 拍板" |
| DD Review(52) | 多人 | Tech Lead 1 人 + aux-10 checklist 自评 | 90 分通过 |
| UAT(90-95) | 客户主导 | 内部 smoke + 团队 1 人扮用户 | 走 OT(88)替代 |
| ITa / ITb(69/70) | 内外分两阶段 | 合并为"API 集成"(71) | 跳过独立外部阶段 |
| ST(76-89) | 14 类 | 简化为 4 类:功能 / 性能 / 安全 / 部署 | 详见 Day 1 任务清单 |
| 12 个 GATE | 12 个独立评审 | **4 个关键 GATE**(见 1.4) | PR 合入 / 单测 / Smoke / 部署 |
| 14 文档模板 | 全部 | 必填 5 + 选填 9 | 必填见 §4 |
| 13 份 aux | 全部 | 全部保留(不省) | 这些是质量底座 |

---

## 4. Day 1 必须填的文档(必填 5 份)

| 编号 | 文档 | 责任方 | 何时 | 状态(2026-08-23) |
|---|---|---|---|---|
| 1 | `05-project-charter.md` | PM (Tech Lead 兼) | Day 1 | 待填 |
| 2 | `131-project-plan.md` | PM | Day 1 | 待填 |
| 3 | `aux-01-naming-convention.md` | Tech Lead | Day 1 | ✅ 已填实 v1.1.0 |
| 4 | `aux-03-error-code-registry.md` | Tech Lead | Day 1 | ✅ 已填实 v1.1.0 |
| 5 | `aux-13-protocol-frame-samples.md` | Tech Lead | Day 1 | ✅ 已填实 v1.1.0 |

**选填**(有了就更好):
- `04-product-proposal.md`(产品 PRD) — 待填
- `07-as-is-system-architecture.md`(技术债清单) — 待填
- `14-nfr-matrix.md`(性能数字) — 待填(POC-01/02 后回填)
- `aux-02-data-dictionary.md`(数据字典) — ✅ 已填实 v1.1.0(14 张表完整)
- `aux-12-config-spec.md`(配置项) — 引用 `ImplementationSpec.md §6`(23 个变量已列)

**2026-08-23 设计文档完善批次**(Mavis 辅助):

| 文档 | 变更 |
|---|---|
| `BasicDesign.md §14` | 补全"配置与密钥管理"完整章节(原被截断):配置分层 / 启动期校验 / 租户级 settings 字段清单(9 项) / 密钥清单(4 类) / 轮换 6 步流程 / 故障行为 / V1+ 演进 |
| `BasicDesign.md §16` | 扩展遗留决策点:增加 ADR-014/015、settings 字段已落 §14.3 说明 |
| `DetailedDesign.md §9` | 替换 `todo!()` 占位为完整 5 步实现,新增 Identity / Conversation / WsSession / AppError trait 签名 |
| `DetailedDesign.md §10` | 重新组织:必填 9 项 + 可调 14 项 + OTel 3 项 + K3s Secret 8 项,补 `IM_HTTP_PORT` / `IM_GRPC_PORT` / `IM_DB_POOL_MAX_CONNECTIONS` 等 |
| `DetailedDesign.md §11` | 完整追溯矩阵(40+ 行 §11.1 章节-SRS-BD + §11.2 aux + §11.3 下游交付物) |
| `ImplementationSpec.md`(新) | 全新文档,15 章节,MVP 工程实施规范 |
| `aux-01..13` 4 份必填 | 从通用模板改为 IM1.0 专用(术语表 / 14 张表 / 20 项错误码 / 12 类 WS 帧等) |

---

## 5. Day 1 第一个 PR 范围(消息收发)

### 5.1 范围

**功能**:
- 用户注册 / 登录(简单账号密码,无第三方)
- Token 签发 / 校验
- WebSocket 连接 + 鉴权
- 单聊消息发送(POST /api/v1/messages)
- 单聊消息接收(WS push)
- 消息历史查询(GET /api/v1/messages?peer_id=X)
- 消息持久化(单表)

**架构**:
- im-gateway(单一服务,后续再拆)
- SQLite / PostgreSQL(单一数据库,后续再分)
- 无 Redis 缓存(后续再加)
- 无事件总线(直接 DB)

**部署**:
- 本地 dev:`cargo run` + `pnpm dev`
- K3s dev namespace
- 无 staging / prod(后续加)

### 5.2 不做(Out of Scope)

- 群聊 / 频道
- 消息撤回 / 编辑 / 已读
- 文件 / 图片 / 表情
- 离线消息 / 推送通知
- 端到端加密
- 多设备同步
- 后台管理 / 审计

### 5.3 完整任务列表

见 `Day1-Task-List.md`。

---

## 6. 风险登记(初始)

| 编号 | 风险 | 等级 | 缓解 |
|---|---|---|---|
| R-001 | 团队 2-3 人不足,关键路径单人离职将阻塞 | 高 | 文档优先 + 强制 CR(2 人均需 approve) |
| R-002 | 简化流程放过非功能问题(安全 / 性能) | 中 | aux-10 / aux-07 不省;关键路径加 SAST |
| R-003 | 极简 RACI 让"谁负责"模糊 | 中 | 写 `Workflow-RACI.md`,有据可依 |
| R-004 | GitHub 在国内访问慢 | 低 | 镜像用 GHCR;CI 用 GitHub Actions 海外节点 |
| R-005 | K3s + Rust 技术栈学习曲线 | 中 | 优先招聘有 Rust 经验者;不熟悉者先读 std + tokio |

---

## 7. 立即行动(Day 1 启动清单)

- [ ] 决定:GitHub 组织名(创建仓库)
- [ ] 决定:里程碑日期(见 §2.1)
- [ ] 创建仓库 `im1.0`(主仓库)
- [ ] 创建 `.github/workflows/ci.yml`(GitHub Actions)
- [ ] 创建 `docs/Project-Status.md` ✓(本文档)
- [ ] 创建 `docs/Workflow-RACI.md`(RACI 矩阵)
- [ ] 创建 `docs/Day1-Task-List.md`(Day 1 任务)
- [ ] 创建 `docs/Platform-Specifics.md`(GitHub / K3s 配置)
- [ ] 第一个 PR 提交通道开

---

**维护**:任何决策变更须更新本文档;每周 review 一次。
