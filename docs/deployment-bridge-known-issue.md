---
doc_id: 133-aux-f1
title_zh: WBS F-1 Docker Desktop Daemon Bridge 已知问题
phase: 15-management
activity_no: 133
owners: 架构师 (Mavis 接手 agent per DEC-008)
status: Blocked
version: 1.0.0
---

# 133-aux-f1. F-1 Docker Desktop Daemon Bridge 已知问题

> 上游: `docs/132-wbs.md` v1.0.0 §5.6 F-1
> 责任方: 架构师 (Mavis 接手 agent per DEC-008) 代 Ulysses 排查
> 修复: **Blocker — 需 Ulysses 手动重启 Docker Desktop**
> 编制日: 2026-09-01 JST

## 1. 现象

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
