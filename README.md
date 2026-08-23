# IM1.0

IM 通信软件，适合 AI 和工作场景，便于集成进游戏的通信软件。

以 IM 即时通信为核心，游戏为首要垂直场景，向 AI 与工作场景自然扩展的可嵌入式实时通信平台。技术栈以 Rust 为主、Python 为辅（限 AI 扩展场景），前端使用 Next.js，部署基于 K3s，全部依赖开源且可商用。

## 文档

- [软件需求规格说明书（SRS）](docs/SRS.md) —— 产品定位、IM Core 与扩展架构、游戏身份桥接、Laser HUD 交互模型、安全与合规、MVP 与路线图、PoC 计划、ADR 候选、风险登记表。
- [LiveKit 实时语音子系统技术调查与需求定义书](docs/LiveKit-Voice-Subsystem.md) —— LiveKit 架构/License 调查、Voice Control Plane 设计、VoiceScope（Party/Guild/Match/Proximity）、K3s 部署约束、安全威胁建模、PoC 与基准测试计划、最终采纳结论。
- [基本设计书（MVP）](docs/BasicDesign.md) —— 服务拓扑与拆分依据、数据模型、消息 Sequence/幂等设计、Identity/Token API、事件总线 Topic、Game SDK 接入映射、Laser HUD 方案、K3s 部署清单、配置与密钥管理（§14 已补全）。
- [详细设计书（MVP）](docs/DetailedDesign.md) —— 代码仓库结构、内部 gRPC 契约、WebSocket 协议全量定义、REST API 清单、状态机、错误码表、数据库迁移脚本骨架、Rust 模块接口签名（含 5 个核心 trait 完整实现）、配置项清单（23 个变量 + 4 个 K3s Secret Key）、与上游文档追溯矩阵。
- [实施规范 ImplementationSpec](docs/ImplementationSpec.md) —— 基于详细设计的**可执行规范**：MVP 范围声明、Cargo workspace 布局与依赖锁版本、API 实施 Checklist、6 份 DB 迁移完整 SQL（含索引与 CHECK 约束）、错误码 Rust 枚举与 HTTP/gRPC 映射、配置加载与校验、完整 Rust trait 清单（im-core / im-gateway 各模块）、部署 + CI 落地、可观测性 MVP 5 项指标、测试分层与覆盖率目标、安全红线 Checklist、DoD 验收标准、风险与 ADR 候选。**编码时直接对照 §3/§4/§7 三节实现**。
- [Game SDK 集成指南（MVP / Unity）](docs/SDK-Integration-Guide.md) —— 身份接入两种路径、Unity 五步接入代码骨架、离线同步行为、常见接入错误排查、Console/其他引擎现状说明。
- [K3s 部署与运维手册（MVP）](docs/Deployment-Runbook.md) —— 依赖组件安装顺序、数据库迁移、密钥管理与轮换、常见故障排查、扩缩容指引、灾难恢复演练清单。
- [可观测性架构设计](docs/Observability.md) —— 企业级 Observability 设计:现状分析 / 需求 OBS-REQ-NNN / Metrics OBS-MET-NNN / Logs OBS-LOG-NNN / Traces OBS-TRC-NNN / Alert OBS-ALT-NNN / SLO OBS-SLO-NNN / 安全 OBS-SEC-NNN / 26 章节 / 6 条 ADR / 完整追踪关系矩阵 / 8 阶段实施路线。
- [软件研发工作流（Workflow）](docs/Workflow.md) —— 150 个工程活动的端到端 SDLC：超上流 / 要件 / 基本 / 详细 / 实现 / 单元 / 集成 / 系统 / 验收 / 迁移 / 发布 / 运维 / 维护 / 品质 / 管理 / 终结，含 GATE、略称表、命名约定、与现有文档的映射。
- [工作流配套文档模板（150 套）](docs/templates/README.md) —— 与 Workflow 1:1 对应：每个工程活动一份模板（含目标 / 责任方 / 输入 / 模板正文 / 验收 / 关联 / 变更记录），按 16 个 Phase 分子目录，附总索引与子目录 README。生成脚本：`scripts/gen_workflow_templates.py`。
- [详细设计阶段辅助模板（13 套）](docs/templates/04-detailed-design/auxiliary/README.md) —— 详细设计阶段跨活动 / 跨切面的支撑文档。**Day 1 必填 4 份（已 2026-08-23 填实，与项目实际设计严格对齐）**：
  - [aux-01 命名规范](docs/templates/04-detailed-design/auxiliary/aux-01-naming-convention.md) —— IM1.0 业务术语单源（`conversation` / `tenant` / `environment` 等 16 项）+ Rust/TS/DB/API/协议命名 + Core Schema 禁游戏专有字段红线
  - [aux-02 数据字典](docs/templates/04-detailed-design/auxiliary/aux-02-data-dictionary.md) —— 14 张表全部字段（类型/必填/范围/隐私级别/脱敏/引用方/留存期/关联 SRS ID）
  - [aux-03 错误码注册中心](docs/templates/04-detailed-design/auxiliary/aux-03-error-code-registry.md) —— 21 项 MVP 错误码（与 `crates/im-common` `ErrorCode` 枚举 + 编译期 `error_code_count_is_21` 测试严格对齐）+ HTTP/gRPC 映射 + 弃用 6 个月流程 + 编译期/CI/运行期三段校验
  - [aux-13 协议帧样例](docs/templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md) —— WS 12 类帧双向、gRPC 4 个核心 RPC、REST 7 个端点、JSON Schema 6 种 kind、wscat/grpcurl/curl 调试命令
  - 其余 9 份（aux-04 状态机、aux-05 CRC、aux-06 算法性能、aux-07 SQL 优化、aux-08 批处理重试 DLQ、aux-09 日志 Cookbook、aux-10 DD Review Checklist、aux-11 时序图、aux-12 配置项）保持模板形态，编码阶段按需填实。生成脚本：`scripts/gen_dd_aux.py`。

## Day 1 启动包（2026-08-20 Kickoff 决议）

> 4 个核心决策已落库：(1) MVP 第一个 PR = 消息收发最小闭环 (2) 产品线 = IM Core (3) 平台 = GitHub + GitHub Actions (4) 团队 = 2-3 人极简。等待填：**关键里程碑日期**。
>
> **技术栈**：Rust 最新 stable + actix-web 4.x + PostgreSQL 18 + sqlx 0.8 + K3s。

- [项目当前状态 Project-Status](docs/Project-Status.md) —— 4 项已决策 + 1 项待决策 + 技术栈选型 + 简化流程对照 + Day 1 必填文档清单 + 风险登记。
- [Workflow RACI 矩阵](docs/Workflow-RACI.md) —— 极简团队下 16 Phase 的 R / A / C / I 分配，含 4 个关键 GATE 与升级路径。
- [Day 1 任务清单](docs/Day1-Task-List.md) —— 14 天任务分解：仓库 / CI / 协议冻结 / 实现 / 联调 / 部署 / 复盘，附验收标准与风险缓解。
- [平台特定配置（GitHub + K3s）](docs/Platform-Specifics.md) —— 仓库结构 / 分支保护 / CODEOWNERS / Secret 配置 / CI/CD YAML / K3s manifests / Dockerfile / 技术栈细节。
