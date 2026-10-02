---
doc_id: 133-aux-f1
title_zh: WBS F-1 Docker Desktop Daemon Bridge 已知问题
phase: 15-management
activity_no: 133
owners: 架构师 (Mavis 接手 agent per DEC-008)
status: Blocked-Intermittent
version: 1.3.0
---

# 133-aux-f1. F-1 Docker Desktop Daemon Bridge 已知问题

> 上游: `docs/132-wbs.md` v1.0.0 §5.6 F-1
> 责任方: 架构师 (Mavis 接手 agent per DEC-008) 代 Ulysses 排查
> 编制日: 2026-09-01 JST
> **状态更新 2026-10-03: Blocker 仍然成立,但已定性为「间歇性」** —— 见 §0。
> **状态更新 2026-10-03 02:00: daemon 再次恢复,已找到无需人工点图标的恢复手段** —— 见 §0.5。
> 下列 §1–§5 保留为 2026-09-01 的原始诊断记录,不代表当前状态。

## 0. 2026-10-02/03 实测:间歇性,非已解除

同一晚内两次观测,结论相反。**此前 v1.1.0 曾据单次观测标为 Resolved,该判断错误,已更正。**

| 时间 (JST) | daemon 状态 | 实证 |
|---|---|---|
| 10-02 ~23:50 | ✅ **UP** | `docker version` Server 29.8.1;`docker pull postgres:18.6` 成功;`im10-pg18b1` 容器正常 Up;7 份 migration 全量应用 7 passed / 0 failed,建成 14 张表;`pg_repos_integration.rs` **22 passed / 0 failed**(10.26s);`diag-docker-bridge.ps1` exit 0 |
| 10-03 ~00:45 | ❌ **DOWN** | Docker Desktop 全部进程消失;`docker` 报 `npipe:////./pipe/dockerDesktopLinuxEngine ... system cannot find the file specified`;`diag-docker-bridge.ps1` exit 1;同期 `pg_repos_integration.rs` 21 failed + `message_service_test.rs` 7 failed,全部 `PoolTimedOut`(容器随 daemon 一起消失) |

### 0.1 定性

F-1 **不是"已修复",而是"时好时坏"**。daemon 会在运行数十分钟后自行消失,
此时所有依赖 docker 的验证(migration、pg_repos、message_service、
migration_smoke_pg)会**整体失败**,且失败信息是 `PoolTimedOut` /
连接被拒 —— 极易被误判成代码缺陷。

排查口诀(下次再见到 `PoolTimedOut` 批量失败,先查这个):
1. `pwsh scripts/diag-docker-bridge.ps1` —— exit 0 / exit 1;
2. exit 1 就是 daemon 掉了,先按 §3 重启 Docker Desktop,再重跑测试。

### 0.2 诊断脚本已修,且经双向验证

`scripts/diag-docker-bridge.ps1` 的判据原有两处缺陷,叠加后使该脚本
**从不可能报出 OK**(`Select-Object -First 15` 把 `Server Version:` 所在的
第 ~62 行截掉;正则 `Server:\s*Version` 又匹配不到隔着非空白行的
`Server Version:`)。已于 2026-10-02 修复(不截断 + 匹配
`^\s*Server Version:\s*\S+`)。

修复后两个方向都验证过:**daemon 活着 → exit 0;daemon 真死 → exit 1**。
修之前这两种情况都报 `[BLOCKED]`,该脚本在 Blocker 期间实际没有诊断价值。

> 注:§4 曾记录"per 强约束 #5 不再重试 2 次以上"。该约束针对**自动化
> 重启 daemon**;手工按 §3 重启 Docker Desktop 仍适用。

### 0.3 已解锁的成果(daemon 可用期间取得,结论有效)

以下结论在 daemon 正常时**实测成立**,不因 daemon 再次消失而失效:

- 7 份 `migrations/*.sql` 全量应用 → **7 passed / 0 failed**,建成 **14 张表**
  (与 ImSpec §1.1 / aux-02 §F.1-F.14 一致)
- `crates/im-core/tests/pg_repos_integration.rs` → **22 passed / 0 failed**
  (im-core 持久化层首次在真 PG 上跑通)
- 新增 `crates/im-gateway/tests/migration_smoke_pg.rs` → **6 passed / 0 failed**
- F-2 / F-3 / F-4 仍**受本 Blocker 约束**(K3s dev 依赖 docker 部署)

### 0.4 对关键路径的影响(Blocker 仍成立)

```
H-1 → H-3 → C-1 → C-2 → C-9 → C-11 → D-3 → E-3 → F-2 → F-3
       ↓
      B-1 ──────(依赖 F-1)
```

- **B-1**:已完成(见 §0.3),后续回归验证在 daemon 稳定前需手工重跑
- **F-2 / F-3 / F-4**:仍被 F-1 阻塞,需 daemon 能稳定保持

### 0.5 2026-10-03 02:00 再次恢复:无需人工点图标的恢复手段

**这次 daemon 恢复不需要 Ulysses 手动点开始菜单。** 已授权 Mavis 直接执行:

```powershell
Start-Process "C:\Program Files\Docker\Docker\Docker Desktop.exe"
```

约 1 分钟后 daemon 就绪, `docker info` 返回 `Server Version: 29.8.1`,
`diag-docker-bridge.ps1` exit 0 并报 `[OK] Docker daemon 已就绪`。

容器 `im10-pg18b1` 会随之变成 `Exited (255)`, `docker start im10-pg18b1` 即可恢复
(端口 5544, `im/im/im_test`, 数据卷保留); 7 份 migration 幂等重放后 **14 张表**在位。

> **F-1 的处置成本已从「需人工介入」降为「一条命令 + 约 1 分钟」**。
> 但这只是**恢复手段**, 不是**根因修复** —— daemon 为什么会毫无征兆地全部
> 进程消失仍未定位。`status: Blocked-Intermittent` 不变。

### 0.6 排查该问题时, 诊断脚本自身暴露的两处缺陷

用 `diag-docker-bridge.ps1` 排查 10-03 的 F-1 复现时, 脚本**自己**先失效了,
两次把我卡住。已修 (`ca6f89f`), 记录在此以免复发:

1. **脚本自身会无限挂死**。第 1 段 `& $dockerExe version` 没有超时保护, 而
   F-1 下 `docker.exe` 本就无限阻塞在 named pipe 连接上。此前只给第 6 段
   (`docker info`) 加了 `Wait-Job` 兜底, 漏了第 1 段。已抽出 `Invoke-Docker`
   统一包装(Start-Job + Wait-Job + 超时 Stop-Job), 第 1、6 段共用。
   修后: daemon 故障时 23.7s 跑完(3×8s), 不再挂死。

2. **WSL distro 判据恒假**。输出里明明是 `docker-desktop    Running         2`,
   下一行却打 `[FAIL] NOT Running`。根因: `wsl -l -v` 在 Windows 上走 UTF-16LE,
   被 PowerShell 当字节流捕获后每行夹着 **NUL 字节**, 正则
   `docker-desktop\s+Running` 的 `\s+` 匹配不上 NUL。不止误报一行 ——
   判定末支依赖 `$dockerDesktopRunning`, 它恒为 false 时「distro 在跑但
   `docker info` 慢」那一支永远走不到。修法: 匹配前剥掉 NUL 与回车。
   修后同环境正确报 `[OK] docker-desktop WSL distro is Running`。

**教训**: 诊断工具在**故障态**下的行为必须单独验证。只验「正常时能跑通」会
漏掉「故障时自己挂死」这类缺陷 —— 而后者正是最需要它的时刻。

## 1. 现象(2026-09-01 记录)

`docker ps` 在 Windows PowerShell 端**2+ 分钟超时**,但 Docker Desktop.exe 进程已启动,WSL `docker-desktop` distro 已 Running。

## 2. 根因(已诊断)

| 检查项 | 状态 | 备注 |
|---|---|---|
| Docker CLI | OK 29.7.2 | `docker context ls` 正常 |
| Docker Desktop.exe | Running | 5 个进程组(Docker Desktop × 4 + com.docker.backend × 2 + com.docker.build + docker) |
| WSL `docker-desktop` distro | Running | `wsl -l -v` 显示 Running |
| `com.docker.service` (Windows service) | Stopped (StartType=Manual) | 未开机自启 |
| Named pipe `\\.\pipe\dockerDesktopLinuxEngine` | **未生成** | 这是 daemon 暴露给 Windows CLI 的桥 |
| `docker info` | TIMEOUT 5-8s | 永远停在 Client 段,Server 段报 `failed to connect` |

**核心矛盾**:WSL `docker-desktop` distro Running ≠ daemon pipe 就绪。
Docker Desktop 在 WSL2 后端模式下,daemon 实际跑在 `docker-desktop` distro 内的 Linux 进程,
而 Windows 端 CLI 通过 `\\.\pipe\dockerDesktopLinuxEngine` 跟它通讯;此 pipe 由
`com.docker.backend` 进程负责建立。当 `com.docker.service` 没正常起来时,pipe
不会生成,CLI 就一直超时。

## 3. 修复路径(Ulysses 手动)

1. 退出 Docker Desktop 全部进程:
   ```powershell
   Get-Process | Where-Object { $_.ProcessName -match 'docker|Docker' } | Stop-Process -Force
   ```
2. 等 10s
3. 开始菜单启动 "Docker Desktop"
4. 等右下角托盘图标变绿(**首次启动 1-2min**,日常重启 ~30-60s)
5. 跑诊断脚本确认:
   ```powershell
   pwsh scripts/diag-docker-bridge.ps1
   ```
   预期:`[OK] Docker daemon 已就绪`,exit code 0

## 4. 自动化尝试记录(均失败,不再重试 per 强约束 #5)

| # | 尝试 | 结果 |
|---|---|---|
| 1 | `Start-Service com.docker.service` | `Cannot open 'com.docker.service' service on computer '.'` — 服务在当前 session 没权限或注册表丢失 |
| 2 | `Start-Process "Docker Desktop.exe"` 后等 90s | 进程在跑,WSL docker-desktop Running,但 pipe 仍未生成 |
| 3 | `wsl -d docker-desktop -- docker ps` | Docker Desktop WSL2 后端**禁止**直接 CLI 调用,返回 "It looks like you have tried to invoke the docker CLI from the docker-desktop WSL2 distribution. This is not supported." |
| 4 | `DOCKER_HOST=npipe:////./pipe/docker_engine` 切到 default context | 同样 `The system cannot find the file specified.` |

> 决策: per WBS 任务强约束 #5 "修复 F-1 失败时,不要重试 2 次以上",不再尝试。

## 5. 对 WBS 关键路径的影响

```
H-1 → H-3 → C-1 → C-2 → C-9 → C-11 → D-3 → E-3 → F-2 → F-3
       ↓
      B-1 ──────(依赖 F-1)
```

- **F-1**: Blocker,见 §3 修复路径
- **B-1**: **不受 blocker 影响** — WSL Ubuntu 上 PostgreSQL 18.6 已在 127.0.0.1:5432 运行,
  `psql 18.6 (Ubuntu 18.6-1.pgdg24.04+2)` 可用,可在 WSL 内直接跑 `migrations/*.sql`。
  B-1 完成不依赖 F-1。
- **F-2** (K3s dev namespace):依赖 F-1,F-1 修复后才能跑
- **F-3** (CI deploy-dev):依赖 F-2,顺延
- **F-4** (healthz/readyz):依赖 F-3,顺延

## 6. 关联交付物

- `scripts/diag-docker-bridge.ps1` — 6 步诊断,exit code 0/1/2
- `docs/132-wbs.md` §5.6 F-1 status = Blocked(等本文件落地)
- 后续:`docs/133-progress-report.md` 跟踪 F-1 修复进展(待 Ulysses 重启 Docker Desktop 后填)

## 7. 修订记录

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 初版:诊断 + 修复路径 + WBS 影响 + B-1 不受影响说明 |
| 1.1.0 | 2026-10-02 | 架构师 (Mavis 接手 agent per DEC-008) | 曾据单次观测标 Resolved。实测 7/7 migration + 14 表 + pg_repos 22/22;定位并修复 `diag-docker-bridge.ps1` 两处判据缺陷 |
| 1.2.0 | 2026-10-03 | 架构师 (Mavis 接手 agent per DEC-008) | **更正 1.1.0 的错误结论**:同夜 00:45 Docker Desktop 全部进程消失,诊断 exit 1,同期 pg_repos / message_service 批量 `PoolTimedOut`。F-1 定性为 **Blocked-Intermittent**(非已解除),Blocker 仍成立。补排查口诀,避免再把 daemon 掉线误判成代码缺陷 |
