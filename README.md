# IM1.0

IM 通信软件，适合 AI 和工作场景，便于集成进游戏的通信软件。

以 IM 即时通信为核心，游戏为首要垂直场景，向 AI 与工作场景自然扩展的可嵌入式实时通信平台。技术栈以 Rust 为主、Python 为辅（限 AI 扩展场景），前端使用 Next.js，部署基于 K3s，全部依赖开源且可商用。

## 接入本平台（Integrator）

**最快的方式**：下载 `im1.0-<版本>-win-x64.zip`（由 `scripts/build-package.ps1` 组装），解压后

```powershell
pwsh -File scripts\install.ps1     # 生成 .env -> 配置自检 -> 跑数据库迁移
pwsh -File scripts\start-gateway.ps1
```

包内是预编译的 `im-gateway` / `im-migrate` / `jobctl`，**不需要 Rust 工具链、不需要 Docker**。`BUILD-INFO.txt` 记录了它是从哪个 commit 构建的，`SHA256SUMS.txt` 可逐文件校验。

**没有安装包时**（或要跑 Linux/macOS）：[API 快速接入](docs/api/QUICKSTART.md) —— 五分钟走通「注册 guest 身份 → 建会话 → 发消息 → 开 WebSocket」，含端点全表、必需环境变量、**以及哪些端点还没实现**（免得照着文档打过去拿到 404）。

机器可读契约：[OpenAPI 3.1](docs/api/openapi.json) ｜ [AsyncAPI 3.0](docs/api/asyncapi.json)。
两者与代码之间有漂移门禁，`docs/api/QUICKSTART.md` 的端点表也被
`scripts/check-api-quickstart.ps1` 与 `openapi.json` 双向对拍。

> 配置自检：`scripts/preflight.ps1` 会在启动前逐项给出可操作的结论，**任何情况下都不打印变量的值**。它之所以必要，是因为 `im-gateway` 配置出错时只会打印一句 `config load failed: internal error` —— 见 `packaging/template/INSTALL.md` §5 的两条实测原因。

## 文档

- [软件需求规格说明书（SRS）](docs/SRS.md) —— 产品定位、IM Core 与扩展架构、游戏身份桥接、Laser HUD 交互模型、安全与合规、MVP 与路线图、PoC 计划、ADR 候选、风险登记表。
- [LiveKit 实时语音子系统技术调查与需求定义书](docs/LiveKit-Voice-Subsystem.md) —— LiveKit 架构/License 调查、Voice Control Plane 设计、VoiceScope（Party/Guild/Match/Proximity）、K3s 部署约束、安全威胁建模、PoC 与基准测试计划、最终采纳结论。
- [基本设计书（MVP）](docs/BasicDesign.md) —— 服务拓扑与拆分依据、数据模型、消息 Sequence/幂等设计、Identity/Token API、事件总线 Topic、Game SDK 接入映射、Laser HUD 方案、K3s 部署清单、配置与密钥管理（§14 已补全）。
- [详细设计书（MVP）](docs/DetailedDesign.md) —— 代码仓库结构、内部 gRPC 契约、WebSocket 协议全量定义、REST API 清单、状态机、错误码表、数据库迁移脚本骨架、Rust 模块接口签名（含 5 个核心 trait 完整实现）、配置项清单（23 个变量 + 4 个 K3s Secret Key）、与上游文档追溯矩阵。
- [实施规范 ImplementationSpec](docs/ImplementationSpec.md) —— 基于详细设计的**可执行规范**：MVP 范围声明、Cargo workspace 布局与依赖锁版本、API 实施 Checklist、8 份 DB 迁移完整 SQL（含索引与 CHECK 约束）、错误码 Rust 枚举与 HTTP/gRPC 映射、配置加载与校验、完整 Rust trait 清单（im-core / im-gateway 各模块）、部署 + CI 落地、可观测性 MVP 5 项指标、测试分层与覆盖率目标、安全红线 Checklist、DoD 验收标准、风险与 ADR 候选。**编码时直接对照 §3/§4/§7 三节实现**。
- [Game SDK 集成指南（MVP / Unity）](docs/SDK-Integration-Guide.md) —— 身份接入两种路径、Unity 五步接入代码骨架、离线同步行为、常见接入错误排查、Console/其他引擎现状说明。
- [K3s 部署与运维手册（MVP）](docs/Deployment-Runbook.md) —— 依赖组件安装顺序、数据库迁移、密钥管理与轮换、常见故障排查、扩缩容指引、灾难恢复演练清单。
- [可观测性架构设计](docs/Observability.md) —— 企业级 Observability 设计:现状分析 / 需求 OBS-REQ-NNN / Metrics OBS-MET-NNN / Logs OBS-LOG-NNN / Traces OBS-TRC-NNN / Alert OBS-ALT-NNN / SLO OBS-SLO-NNN / 安全 OBS-SEC-NNN / 26 章节 / 6 条 ADR / 完整追踪关系矩阵 / 8 阶段实施路线。
- [软件研发工作流（Workflow）](docs/Workflow.md) —— 150 个工程活动的端到端 SDLC：超上流 / 要件 / 基本 / 详细 / 实现 / 单元 / 集成 / 系统 / 验收 / 迁移 / 发布 / 运维 / 维护 / 品质 / 管理 / 终结，含 GATE、略称表、命名约定、与现有文档的映射。
- [工作流配套文档模板（150 套）](docs/templates/README.md) —— 与 Workflow 1:1 对应：每个工程活动一份模板（含目标 / 责任方 / 输入 / 模板正文 / 验收 / 关联 / 变更记录），按 16 个 Phase 分子目录，附总索引与子目录 README。生成脚本：`scripts/gen_workflow_templates.py`。
- [详细设计阶段辅助模板（13 套）](docs/templates/04-detailed-design/auxiliary/README.md) —— 详细设计阶段跨活动 / 跨切面的支撑文档，**13 份全部已填实**（每份都带自己的变更记录，逐份填实进度见其 §K / §L）：
  - [aux-01 命名规范](docs/templates/04-detailed-design/auxiliary/aux-01-naming-convention.md) —— IM1.0 业务术语单源（`conversation` / `tenant` / `environment` 等 **18** 项）+ Rust/TS/DB/API/协议命名 + Core Schema 禁游戏专有字段红线
  - [aux-02 数据字典](docs/templates/04-detailed-design/auxiliary/aux-02-data-dictionary.md) —— **15** 张表全部字段（类型/必填/范围/隐私级别/脱敏/引用方/留存期/关联 SRS ID）
  - [aux-03 错误码注册中心](docs/templates/04-detailed-design/auxiliary/aux-03-error-code-registry.md) —— 21 项 MVP 错误码（与 `crates/im-common` `ErrorCode` 枚举 + 编译期 `error_code_count_is_21` 测试严格对齐）+ HTTP/gRPC 映射 + 弃用 6 个月流程 + 编译期/CI/运行期三段校验
  - [aux-06 算法性能模型](docs/templates/04-detailed-design/auxiliary/aux-06-algorithm-performance-model.md) —— 13 项关键算法的复杂度与目标；**§D 已回填 3 项实测**（A-001 / A-008 解析半段 / A-010），其余按缺失基础设施分类写明
  - [aux-08 批处理重试与 DLQ](docs/templates/04-detailed-design/auxiliary/aux-08-batch-retry-dlq.md) —— 重试/DLQ 协议、`dlq_records` 处置流程（配套 `jobctl dlq list|replay|discard`）
  - [aux-13 协议帧样例](docs/templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md) —— WS 客户端 8 类 / 服务端 10 类帧、gRPC 核心 RPC、REST 端点（**当前 24 个 operation**，以 `openapi.json` 为准）、JSON Schema 6 种 kind、wscat/grpcurl/curl 调试命令
  - 其余 **7** 份（aux-04 状态机、aux-05 CRC、aux-07 SQL 优化、aux-09 日志 Cookbook、aux-10 DD Review Checklist、aux-11 时序图、aux-12 配置项）同样已填实。生成脚本：`scripts/gen_dd_aux.py`。

## 本地开发环境

> 下列三项是**实际踩过的坑**，不是预防性建议。详细排查记录见
> [Project-Status](docs/Project-Status.md) §1.1.6 / §1.1.7 与
> [F-1 已知问题](docs/deployment-bridge-known-issue.md)。

### 1. `protoc` 是必需的

`crates/im-proto` 的 `build.rs` 走 `tonic-build` → `prost-build`，需要 `protoc`
可执行文件。**缺了它，任何编译 workspace 的命令都会失败**：

```
Error: Custom { kind: NotFound, error: "Could not find `protoc` ..." }
```

Linux：`apt-get install -y protobuf-compiler`（CI workflow 里已装）。
Windows：装好后确保 `protoc` 在 `PATH`，或设 `PROTOC` 环境变量指向它。

> 常见误判：本地装了 protoc 所以一切正常，CI 却红。**换机器就是换环境**，
> 判断"能不能过"要看目标环境的记录，不能看本机。

### 2. 内存受限，限制并行度

全 workspace 构建 / 测试在 8–16 GB 内存的机器上**必须限并行度**，否则会在链接
阶段被系统杀掉（表现为无明确报错的失败）：

```powershell
cargo test --workspace --lib --no-fail-fast -j 1
```

OOM 后的报错具有**极强误导性**——`ring` 版本冲突、`rlib format not found`
之类都只是内存耗尽的连锁症状，**不要因此去改 `Cargo.toml` 的依赖版本**。
清残留进程后重跑通常自愈：

```powershell
Get-Process | Where-Object { $_.ProcessName -match '^(cargo|rustc|cargo-clippy|clippy-driver)$' } |
  ForEach-Object { try { Stop-Process -Id $_.Id -Force -ErrorAction Stop } catch {} }
```

### 3. Windows 下 `.ps1` 必须是 UTF-8 with BOM

Windows PowerShell 5.1 读取**无 BOM** 的 `.ps1` 时按系统 ANSI 代码页
（简体中文 Windows = GBK/CP936）解码。GBK 双字节的第二字节合法范围含 ASCII
符号位（`0x40-0x7E`），于是中文字符的字节被误判为前导字节，**吞掉紧随其后的
引号**，脚本直接 ParserError 崩，且报错（`MissingEndCurlyBrace`、「字符串缺少
终止符」）完全指不到真实原因。PowerShell 7 默认按 UTF-8 读，所以 7 下正常。

**新增或修改任何 `.ps1` 时保存为 UTF-8 with BOM。** 仓库有 lint 兜底（CI
`sast` job），会拒绝「含非 ASCII 字节却缺 BOM」的文件：

```powershell
pwsh scripts/lint-ps1-encoding.ps1
```

### 常用命令

```powershell
# 单元测试（不依赖 PG）
cargo test --workspace --lib -j 1

# 需要 PostgreSQL 的集成测试：设 DATABASE_URL 后跑
$env:DATABASE_URL = "postgres://im:im@localhost:5432/im_test"
cargo test -p im-core --test pg_repos_integration -j 1
cargo test -p im-core --test message_service_test    -j 1
cargo test -p im-gateway --test migration_smoke_pg    -j 1
# 未设 DATABASE_URL 时上述 PG 用例会**静默跳过**(libtest 眼里「跳过」=「通过」,
# 输出上看不出来)。想让它在缺库时**明确失败**, 加 IM_REQUIRE_PG=1:
#   $env:IM_REQUIRE_PG = "1"
# CI 的两个 test job 都设了它 —— 所以「CI 绿」等于「PG 用例真的跑了」。

# 性能基准(不参与 CI, 需手动跑; aux-06 §D 的数字来自这里)
cargo bench -p im-core --bench auth_hotpath -- --warm-up-time 1 --measurement-time 2 --sample-size 20
cargo bench -p im-protocol --bench ws_frame_parse

# 格式与静态检查（与 CI 门禁一致）
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
# 内存紧张时限并行度: -j N 必须放在 -- 之前
# 写成 `-- -D warnings -j 1` 会被当作 clippy-driver 的参数, 报
# `error: Unrecognized option: 'j'`(exit 101), 看着像代码坏了其实是命令行错
# cargo clippy --workspace --all-targets --locked -j 1 -- -D warnings

# 契约漂移门禁（纯 PowerShell, 不需要 cargo）
pwsh scripts/check-openapi.ps1          # spec vs 路由表 vs 错误码注册中心
pwsh scripts/check-asyncapi.ps1         # WS 帧 vs im-protocol 的 serde 定义
pwsh scripts/check-api-quickstart.ps1   # 接入文档的端点表 vs openapi.json
pwsh scripts/check-error-codes.ps1      # aux-03 §B vs im_common::ErrorCode
pwsh scripts/check-naming-convention.ps1
pwsh scripts/test-preflight.ps1         # 配置自检脚本的变异测试 + 密钥泄露断言

# 组装 release 安装包（预编译二进制 + 配置模板 + 启动脚本 + 校验和）
pwsh scripts/build-package.ps1          # 产物在 dist/，含 BUILD-INFO.txt 与 SHA256SUMS.txt

# 诊断脚本（Docker 故障时用，有超时保护，不会自己卡死）
pwsh scripts/diag-docker-bridge.ps1
```

## Day 1 启动包（2026-08-20 Kickoff 决议）

> 4 个核心决策已落库：(1) MVP 第一个 PR = 消息收发最小闭环 (2) 产品线 = IM Core (3) 平台 = GitHub + GitHub Actions (4) 团队 = 2-3 人极简。等待填：**关键里程碑日期**。
>
> **技术栈**：Rust 最新 stable (1.98.0) + actix-web 4.x + PostgreSQL 18.6 + sqlx 0.9 + K3s。

- [项目当前状态 Project-Status](docs/Project-Status.md) —— 4 项已决策 + 1 项待决策 + 技术栈选型 + 简化流程对照 + Day 1 必填文档清单 + 风险登记。
- [Workflow RACI 矩阵](docs/Workflow-RACI.md) —— 极简团队下 16 Phase 的 R / A / C / I 分配，含 4 个关键 GATE 与升级路径。
- [Day 1 任务清单](docs/Day1-Task-List.md) —— 14 天任务分解：仓库 / CI / 协议冻结 / 实现 / 联调 / 部署 / 复盘，附验收标准与风险缓解。
- [平台特定配置（GitHub + K3s）](docs/Platform-Specifics.md) —— 仓库结构 / 分支保护 / CODEOWNERS / Secret 配置 / CI/CD YAML / K3s manifests / Dockerfile / 技术栈细节。
