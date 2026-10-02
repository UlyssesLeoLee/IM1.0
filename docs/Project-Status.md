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
| 1.2.0 | 2026-10-02 | 架构师 (Mavis 接手 agent per DEC-008) | 新增 §1.1.2 CI 三闸门实测基线(fmt / clippy / test 全部 exit 0)+ CI 分支触发修复记录 + 本机 toolchain 缺陷记录;新增 `dangling-references.md` 关联 |
| 1.3.0 | 2026-10-03 | 架构师 (Mavis 接手 agent per DEC-008) | 新增 §1.1.3 B-1 结清(7/7 migration + 14 表 + 22 个真 PG 集成测试 + 6 个新 schema 约束测试)。**更正 1.2.0 暗示的"F-1 已解除"**:同夜 Docker daemon 消失,F-1 定性为 Blocked-Intermittent,Blocker 仍成立 |
| 1.4.0 | 2026-10-03 | 架构师 (Mavis 接手 agent per DEC-008) | 新增 §1.1.4 移除 17 处 blanket lint 压制(修 30 个真实 error, clippy 改为真干净)、§1.1.5 补 `gap-ledger.md` 缺口台账文档 |
| 1.5.0 | 2026-10-03 | 架构师 (Mavis 接手 agent per DEC-008) | 新增 §1.1.6 WS 心跳超时**实际不生效**(功能缺陷, 文档与实现不符)+ ps1 编码修复(10 个 .ps1 在 Windows PowerShell 5.1 下 ParserError)。新增 `scripts/lint-ps1-encoding.ps1` 并挂进 CI `sast` job |
| 1.6.0 | 2026-10-03 | 架构师 (Mavis 接手 agent per DEC-008) | 新增 §1.1.7 **CI 4 个 job 全红**(ubuntu runner 缺 protoc, 致 im-proto 编译失败, 已修)。**更正 §1.1.2**: 该节标题原标 `[GATES-GREEN]`, 实为本机全绿而 CI 一直红, 标题改为 `[LOCAL-GREEN / CI-RED-已修]`; 同时更正其"deploy-dev job 名未确认未修"一条(已于 `56eef9c` 修复) |

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

### 1.1.2 CI 三闸门实测基线 (2026-10-02) [LOCAL-GREEN / CI-RED-已修 2026-10-03]

> **⚠️ 2026-10-03 更正 —— 本节标题原为 `[GATES-GREEN]`,该结论是错的。**
>
> 本节记录的全部数据都是**本机实测**, 属实; 但我把它当成了"CI 也是绿的"并
> 写进了本节标题, 这是错的。dev 上多次 push 的 CI 实为 **4 个 job 全部 failure**,
> 从未被真正验证过。根因是 ubuntu runner 缺 `protoc`(见 §1.1.7),已修。
> **教训: 本机全绿 ≠ CI 全绿** —— 本机有 `C:\protoc\bin\protoc.exe`, 环境差异
> 恰好掩盖了 CI 的失败, 只有 CI 记录本身才是真相。

本节记录 2026-10-02 在 `dev` 分支实测的三道 CI 闸门结果。此前 dev 长期处于
**两道闸门红**的状态(fmt 46 文件违规、clippy 29 errors),无法通过 CI。

**实测结果(dev @ `803bad9`,toolchain cargo/rustc 1.98.1)**:

| 闸门 | 命令 | 修复前 | 修复后 |
|---|---|---|---|
| 格式 | `cargo fmt --all -- --check` | ❌ 46 文件违规 | ✅ **exit 0** |
| 静态分析 | `cargo clippy --workspace --all-targets --locked -- -D warnings` | ❌ exit 101,29 errors | ✅ **exit 0** |
| 单元测试 | `cargo test --workspace --lib --no-fail-fast` | ✅ 128 passed / 0 failed | ✅ 128 passed / 0 failed |

测试构成:im_common 41 / im_core 19 / im_protocol 7 / im_testkit 61;
extension_runtime / im_media / im_presence / im_proto 的 lib tests 为 0。

**本次修复内容**:

- `0e926f3` —— `cargo fmt --all` 统一全仓 46 文件格式;`.gitignore` 补
  `.worktrees/` 与 `target-*/`;`Cargo.lock` 补 `aci-emitter` git dep 条目
  (CI clippy job 用 `--locked`,缺该条目会直接失败)。
- `a26ae67` —— im-gateway 的 29 个 clippy error 清零。其中 12 处死代码
  **标注保留而非删除**(逐项加 `#[allow(dead_code)]` + `守门 #1 缺口台账`
  缺口编号 + 后续接线条件),理由见 §1.1.1 占位符保留约定;未改任何运行时逻辑。
- `efe1768` —— CI 补 `dev` 分支触发 + `sqlx-cli` 版本对齐 `0.9`。
  此前 `ci.yml` / `deploy-dev.yml` 只监听 `main`,而团队实际在 `dev` 上集成,
  **dev 上的真实改动从不跑 CI**(静默系统性缺口)。

**有意未修的项(需团队 lead 拍板)**:

- `deploy-dev.yml` **未**把 `dev` 加入自动部署触发。每次合入 dev 都自动部署
  = 每次合并都对共享 K3s dev 集群跑 apply + 迁移 Job,且
  `concurrency.cancel-in-progress: false` 会让突发合并的部署排队而非取消,
  重叠的迁移 Job 不可重入,存在把共享 dev 环境搞脏的风险。dev 部署保持走
  `workflow_dispatch` 手动触发。
- `deploy-dev.yml` 的 "Run migrations" job 名与 `needs` 依赖不一致
  (建 job `im-migrate-manual` 却固定等 `job/im-migrate`,导致每次 dev 部署必然
  2 分钟超时失败)。**已于 `56eef9c` 修复并用 `bash -n` 校验 exit 0**;
  本条为历史记录,原写于修复前。

**本机 toolchain 缺陷(不影响 CI,属开发者体验问题)**:

`~/.cargo/bin/` 下的 `cargo.exe` / `rustc.exe` shim 丢失(目录不存在),但
`~/.rustup/toolchains/` 下 4 套 toolchain(1.89.0 / 1.95.0 / 1.98 /
1.98.1)完整。因此本机 `cargo` 无法直接调用,需走 toolchain 绝对路径:

```powershell
$env:PATH = "C:\Users\leo19\.rustup\toolchains\1.98.1-x86_64-pc-windows-msvc\bin;$env:PATH"
```

CI 不受影响(`dtolnay/rust-toolchain@stable` 自行安装)。**未修改本机 PATH** ——
恢复 shim 属宿主状态变更,待团队 lead 决定。

**关联**:`dangling-references.md` —— 代码中 46 处 `守门 #N` 引用无定义文档
(`AGENTS.md` 不存在),该文只做事实清单,不定义守则含义。

### 1.1.3 B-1 结清:SQL migration 首次在真 PG 上验证 (2026-10-02) [B-1-CLOSED]

§1.1.1 遗留工程债第 2 条("6 份 SQL migration 未在真 PG 实例上跑过")已结清。

**触发**:F-1(Docker daemon bridge Blocker)当晚曾短暂可用。起 `postgres:18.6`
专用容器(端口 5544,与测试默认值一致)后完成以下验证。

> ⚠️ **2026-10-03 更正**:下表结论在 daemon 正常时**实测成立**,但 F-1 并未
> 解除 —— 同夜 ~00:45 Docker Desktop 全部进程消失,daemon pipe 丢失,同期
> `pg_repos_integration` 与 `message_service_test` 批量 `PoolTimedOut`(容器随
> daemon 一起没了)。F-1 现定性为 **Blocked-Intermittent**,详见
> `docs/deployment-bridge-known-issue.md` v1.2.0。
> **下次见到 `PoolTimedOut` 批量失败,先跑 `pwsh scripts/diag-docker-bridge.ps1`
> 确认 daemon 状态,不要误判成代码缺陷。**

| 验证项 | 结果 |
|---|---|
| 7 份 `migrations/*.sql` 全量应用 | ✅ **7 passed / 0 failed** |
| 建成表数 | ✅ **14 张**(与 ImSpec §1.1 / aux-02 §F.1-F.14 一致) |
| `crates/im-core/tests/pg_repos_integration.rs` | ✅ **22 passed / 0 failed** |
| `crates/im-gateway/tests/migration_smoke_pg.rs`(本次新增) | ✅ **6 passed / 0 failed**(真 PG 18.6) |

`pg_repos_integration.rs` 覆盖 6 个 PgRepository + PgSequenceAllocator +
IdentityService 的 link_account 全流程(6 个 Pg repo:user / device / conversation /
message / reaction / friendship),这是 im-core 持久化层**第一次**在真数据库上跑通。

**本次新增 `crates/im-gateway/tests/migration_smoke_pg.rs`**,补上
`migration_smoke.rs` 明确声明的"不覆盖真实 PG 执行"缺口,覆盖:
14 张表存在性 / 0007 两列存在 / 0007 两条 partial 索引建成 / 0007 的 DB 层
CHECK 约束**确实生效**(非法 username、超长 password_hash 被拒;合法值放行;
同 environment 内 username 唯一;NULL username 可重复)。
**不引入新依赖** —— im-gateway dev-dependencies 已含 `sqlx`(migrate feature)。
`DATABASE_URL` 未设时自动跳过,保证无 PG 的开发机跑全量测试仍全绿;CI 的
test-integration job 已注入 `DATABASE_URL` + `sqlx migrate run`,会自动生效。

> **写这类"DB 约束应该拒绝 X"测试时的陷阱(本项目实测踩到)**:
> `users` 表除 0007 新增的两列外,还有 `kind TEXT NOT NULL` +
> `users_kind_check (kind = ANY(ARRAY['user','guest']))`。
> 若 INSERT 漏给 `kind`,插入会**先**因 NOT NULL 失败 —— 于是
> "非法 username 应被拒""重复 username 应被拒"这类**反向断言会假通过**:
> 它们被拒的原因根本不是 username 约束。
> 修法:所有 `users` 插入收敛到 `insert_user()` 单一入口(强制带 `kind`),
> 并为每条反向断言配一条**正向**断言(合法值必须放行 / 边界值 256 字节
> password_hash 必须放行)—— 若 INSERT 恒失败,正向断言会立刻红。
> 同理 `environments.name` 是白名单 CHECK
> (`name = ANY(ARRAY['production','staging','test'])`),seed 时只能用 `'test'`。

**同时修正的文档缺陷**:

- `migration_smoke.rs` 头部引用 `tests/migration_smoke_docker.rs`
  (feature-gated),但**该文件从不存在** —— 悬空引用,真 PG 覆盖缺口因此长期
  无人补。现已指向真实的 `migration_smoke_pg.rs`。
- `scripts/diag-docker-bridge.ps1` 的 daemon 判据有两处缺陷(① `docker info` 被
  `Select-Object -First 15` 截断,`Server Version:` 在第 ~62 行从未进入结果;
  ② 正则 `Server:\s*Version` 匹配不到中间隔着非空白行的 `Server Version:`)。
  叠加后该脚本**从不可能报出 OK**(daemon 活着也报 `[BLOCKED]`)。两处已修,
  现经**双向验证**:daemon 活着 → exit 0,daemon 真死 → exit 1。
  注意:修好判据 ≠ 修好 daemon —— F-1 本身是**间歇性**的(见 v1.2.0),
  脚本修好后它才第一次能如实反映 daemon 状态。

**仍未验证**:

- `deploy-dev.yml` 的 K3s 实际部署链路从未在真实集群上端到端跑过。F-1 未解除前
  F-2 / F-3 / F-4 不再受此 Blocker 阻塞,可按 `132-wbs.md` 推进。

### 1.1.4 移除 17 处 blanket lint 压制 —— CI 绿灯改为"真干净" (2026-10-03)

§1.1.1 提到 clippy 靠"占位模块加 `#![allow(dead_code, unused_imports,
unused_variables)]`"通过。审计发现**全仓共 17 处这种 blanket 压制** + 1 处
`#![allow(clippy::all)]` —— 也就是说 CI 的 clippy 闸门对绝大部分代码是**空洞的**:
不是代码干净,是警告被静音。

**测量方法**: 在隔离 worktree 里剥掉全部压制后跑
`cargo clippy --workspace --all-targets -- -D warnings`,逐轮修完再看下一轮。

> ⚠️ **测量陷阱(本次踩到)**: clippy 遇到第一个编译失败的 crate 就停止,后面
> 的 crate **根本不会被检查**。第一轮只报出 16 个 im-core error,一度被误读成
> "真实债务只有 16 个且全在 im-core";修完 im-core 后才露出 14 个 im-gateway
> error。**真实总数 30**,必须逐轮修完才能看全。

**真实债务 30 个,分布**:

| 位置 | 数量 | 性质 |
|---|---|---|
| `im-core/src/identity/service.rs` | 5 | 4 未用 import + 1 未读字段 |
| `im-core/src/identity/token.rs` | 3 | 3 未用 import(其中 1 个见下) |
| `im-core/src/identity/repository.rs` | 2 | 未用 import |
| `im-core/src/message/repository.rs` | 2 | 未用 import |
| `im-core/src/{conversation/repository,conversation/service,event/events,message/content}.rs` | 各 1-2 | 未用 import / 未用变量 |
| `im-gateway/src/placeholder.rs` | 10 | 未接线端点桩(文件整体语义) |
| `im-gateway/src/health.rs` + `http/state.rs` | 2 | 未接线 `readyz` / 未读 `tenant_id` |
| **合计** | **30** | |

**处理方式(逐项区分,不做一刀切删除)**:

- **未用 import → 直接删**,但有一个陷阱:`identity/token.rs` 的 `EnvironmentId`
  表面未用,实际在 `#[cfg(test)] mod tests` 内被 `use super::*` 带进来使用。
  直接删会**打断 test 目标编译**。已按 im-gateway 的既有做法移进测试模块。
- **刻意保留的死代码 → 标注不删**,每项写明缺口编号与接线条件:
  `IdentityService::server_secrets`(C-3 S2S 接线)、
  `health::readyz`(F-4 依赖 F-2/F-3)、
  `AuthedUser::tenant_id`(多租户隔离随 G-1/V1)。
- **`placeholder.rs`**: 该文件唯一职责就是存放"已定义但未接线"的端点桩,
  函数按定义不会被调用。保留 `#![allow(dead_code)]`,但**从原先三项收窄到一项** ——
  原先的 `unused_imports` / `unused_variables` 会连带掩盖本文件未来真实的
  import / 变量问题。
- **`im-proto` 的 `#![allow(clippy::all)]` 保留**: 唯一手写代码只是一层
  `include_proto!` 转发, 实质内容全由 tonic-build 生成
  (`OUT_DIR/im.core.v1.rs`), 生成物稳定触发 `result_large_err` 等 lint
  (实测 22 处) 且不受我们控制。已在注释里写明保留理由与收窄待办。

**结果**: 剥掉 17 处压制后
`cargo clippy --workspace --all-targets -- -D warnings` **exit 0**,
`cargo fmt --all -- --check` **exit 0**。CI 的 clippy 绿灯从此不再是压制出来的。

**未修的既有问题(仅记录)**: `message/content.rs` 的
`.map_err(|_| AppError::MessageTooLarge(0, max_bytes))` 把真实 size 丢了
(传的是硬编码 0), 调用方无法知道实际大小。本次只做最小化(消除 unused 变量),
改行为属另一议题。

### 1.1.5 补上「缺口台账」文档 (2026-10-03)

代码里有 26 处注释写「守门 #1 缺口台账: 缺口 #X」, 但 `docs/` 下**从未存在**
这份台账 —— 又一处悬空引用(与 `migration_smoke_docker.rs` 幻影文件、
`AGENTS.md` 缺失同类)。已补 `docs/gap-ledger.md`。

该台账**只汇总代码注释里已有的声明**(缺口 #A..#I + 4 项后续新增), 逐条附
源码位置、注释原文与接线条件, 并画出依赖链:

- **#D/#E/#F/#G/#H/#I 六个全部阻塞在 C-11 (WsSession driver 实装)**, 接线
  C-11 可一次性清掉 —— 这不是六个独立任务, 排期时应按一项算
- `IdentityService::server_secrets` 等 C-3; `readyz` 等 F-2/F-3 → F-1;
  `AuthedUser.tenant_id` 等 G-1/V1

**明确不做的**: 台账**不定义** `守门 #1` 的含义(该编号在本仓库仍无定义文档,
见 `dangling-references.md`), 也**不代替 WBS 排期**, 也**不覆盖** WBS 里那些
没有对应 `#[allow]` 或注释的 `Todo` 项。

**引用链**: 代码注释 → 本文档 §1.1.1(占位符保留约定) → `gap-ledger.md`。

### 1.1.6 两个新发现: WS 心跳超时失效 + ps1 编码缺陷 (2026-10-03)

#### (a) WS 60s 无帧超时**当前不生效**(功能缺陷, 非文档问题)

复核 `im-gateway` WS 心跳时发现, `ws/handler.rs` 模块文档写「30s tick + 60s
无帧超时关闭」, 但实现是:

- 后台 task 在心跳超时时**只** `tracing::info!` + `break`
- 主循环 `run_ws_loop` 只 `await msg_stream.next()`, **无 `select!` 超时分支**
- 而 `actix_ws::Session` 归主循环所有, 后台 task 拿不到

**结果: 60s 无帧超时永远不会关闭连接, 半开连接堆积到 TCP 超时才回收。**
这是一处「文档声称的行为根本没实现」的功能缺陷, 已把 `ws/handler.rs` 模块文档
改为如实描述, 并在 `gap-ledger.md` 把 #H 由「文档不准确」升级为「功能未生效」。

修复需 C-11 (`WsSession` driver) 重构: 用 oneshot/mpsc 把超时信号送给持有
`Session` 的主循环, 或把 `interval` 搬进 `run_ws_loop` 用 `select!`。
**端到端验证依赖 Docker(F-1)**, 当前无法验证。

#### (b) 10 个 .ps1 中 3 个在 Windows PowerShell 5.1 下**直接 ParserError 崩掉**

`scripts/diag-docker-bridge.ps1` 等 3 个含中文的脚本, 开发者随手敲
`powershell scripts\diag-docker-bridge.ps1` 必然崩, 且报错信息
(`MissingEndCurlyBrace` / 「字符串缺少终止符」)完全指不到真实原因。

**根因**: 这些 .ps1 是 UTF-8 **无 BOM**。Windows PowerShell 5.1 读取无 BOM 文件时
按系统 ANSI 代码页(简体中文 Windows = GBK/CP936)解码, 而 **GBK 双字节的第二字节
合法范围含 ASCII 符号位(0x40-0x7E)**, 于是中文字符的字节被误判为 GBK 前导字节,
**吞掉紧随其后的引号**, 字符串提前终止。PowerShell 7 默认按 UTF-8 读, 所以 7 正常。

**修法**: 字节级插入 UTF-8 BOM(EF BB BF), 10/10 文件已修, 5.1 解析错误全部归零。
刻意**不**把中文注释改英文 —— 那会破坏团队可读性, 且 Windows 生态读取
UTF-8 脚本的官方要求本就是带 BOM。

**方法论教训(与 §1.1.4 同源)**: 我最初用「5.1 解析器逐文件扫, err=0 就算好」判定,
结果只抓到 3 个崩的, **漏掉另外 7 个同样是 GBK 隐患的文件** —— 它们只是**碰巧**没踩到
GBK 边界没解析错。所以新增的 lint 检测的是**根因**(含非 ASCII 却无 BOM)而非症状。
症状检测会低估债务, 这与 §1.1.4 里 clippy「逐轮停」导致低估值是同一类陷阱。

**防回归**: 新增 `scripts/lint-ps1-encoding.ps1`(纯字节检查, 跨平台一致, 无需
Windows runner), 挂进 CI `sast` job。实测该 lint 在 pwsh 7 与 5.1 下均 exit 0。

### 1.1.7 CI 4 个 job 全红:ubuntu runner 缺 protoc (2026-10-03) [CI-RED-FIXED]

**这是本项目迄今最严重的一次"绿灯假象", 且由我自己制造。**

#### 事件

查 `gh run list` 发现 dev 上最近多次 push 的 CI 结论是 **failure**, 且
`lint` / `Unit Tests` / `Integration Tests` / `Static Analysis` **四个 job 齐红** ——
而我此前一直汇报"三闸门全绿"。两者并不矛盾: 我的"全绿"是**本机实测**(确实全绿),
但被我当成了 CI 的结论。CI 从未被验证过。

#### 根因

`crates/im-proto` 的 `build.rs` 走 `tonic-build` → `prost-build`, 需要 `protoc`:

```
Error: Custom { kind: NotFound, error: "Could not find `protoc` ..." }
process didn't exit successfully: .../im-proto-.../build-script-build (exit status: 1)
```

GitHub 的 `ubuntu-latest` runner **不预装 protoc**。任何编译 workspace 的 job 都会
经过 im-proto, 故四个 job 全军覆没。

**为什么本机完全掩盖了它**: 本机 `C:\protoc\bin\protoc.exe` (libprotoc 33.4) 存在,
`prost-build` 直接命中。本地 fmt/clippy/test 永远走不到这个失败分支。
**环境差异恰好精确抵消了缺陷** —— 这是"本机全绿"最危险的一种失效模式。

#### 佐证: 团队在别处已经踩过同一个坑

`docker/im-core.Dockerfile` 与 `docker/im-gateway.Dockerfile` 的 builder 阶段
**都已经装了 `protobuf-compiler`** —— 说明"编译 proto 需要 protoc"这件事团队
早就知道并在容器里解决过, 只是**没人把它加进 CI workflow**。缺口只存在于
workflow 这一层, 这反过来印证了根因判断正确。

#### 修法与范围

- `ci.yml` 的 4 个 job 各加一步(装在 cargo 步骤之前):
  `sudo apt-get update -qq && sudo apt-get install -y -qq protobuf-compiler`
- 选 apt 而非第三方 `arduino/setup-protoc` action: 零第三方依赖, 信任面最小
- **`release.yml` 无需改**: 它走 docker build, Rust 编译在容器内完成,
  protoc 已由 Dockerfile 提供; `im-migrate` 只 COPY `sqlx-cli` 二进制, 不编译 Rust

#### 顺带修: `im-migrate.Dockerfile` 的 sqlx 版本不一致

该文件仍是 `sqlx-cli --version '^0.8'`。§1.1.2 记录的 lane 2 修复只把 `ci.yml`
对齐成 `^0.9`, 这里漏了 —— 于是**CI 用 sqlx 0.9 验证过的 migration, 到 K3s 上却由
sqlx 0.8 执行**。现已与 `ci.yml` 及 workspace 的 `sqlx = "0.9"` 三处对齐。

#### 方法论: "本机全绿"不是证据, CI 记录才是

本项目已第三次在**测量/验证方法**上栽跟头, 每次形态不同:

| 次数 | 陷阱 | 低估了什么 |
|---|---|---|
| §1.1.4 | clippy 遇首个编译失败 crate 即停 | 真实 lint 债务 30 个被报成 16 个 |
| §1.1.6(b) | 用「解析器 err=0」代替「检测根因」 | 10 个 GBK 隐患只抓到 3 个崩的 |
| §1.1.7 | 用「本机 exit 0」代替「CI 结论」 | 4 个 job 全红被当成全绿 |

**共同形状**: 拿一个**更窄的**代理指标(能编译 / 能解析 / 本机能过)当结论,
而真正要回答的问题(债务总量 / 是否隐患 / 换台机器还成立)从没被直接测量。
**对策**: 结论必须由**与结论同环境**的证据支撑 —— 债务要逐轮测干净,
编码要测根因, 门禁要看 CI 记录本身。

#### 待评估(未做): vendored protoc

根治方案是让 `im-proto` 用 `protoc-bin-vendored` 内联 protoc, 一处修复同时覆盖
CI / 本机 / 任意开发者环境 / Dockerfile, 达成可复现构建。**本次未做**, 因为它要改
`build.rs` + `Cargo.toml` + `Cargo.lock`(CI 用 `--locked`), 触及构建链路, 而当前
F-1(Docker 死)导致无法完整验证回归。属需要 lead 拍板的选型, 记录待办。

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
