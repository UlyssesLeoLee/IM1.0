#!/usr/bin/env pwsh
# ============================================================================
# diag-docker-bridge.ps1
# 用途:F-1 已知问题诊断。检查 Docker Desktop (WSL2 后端) daemon 状态,
#       给出修复指引(用户需手动操作)。
#
# 用法:pwsh scripts/diag-docker-bridge.ps1
# 输出:stdout + $LASTEXITCODE (0=OK, 1=bridge-broken, 2=daemon-not-running)
#
# 背景(per 132-wbs.md F-1):
#   当前 `docker ps` 在 2+ 分钟超时,根因是 Docker Desktop WSL2 后端
#   (`docker-desktop` WSL distro) 启动后,`com.docker.service` 没成功把
#   `\\.\pipe\dockerDesktopLinuxEngine` named pipe 暴露给 docker CLI。
#
# 已知触发条件(Ulysses 2026-09-01 环境):
#   - com.docker.service StartType=Manual,未开机自启
#   - Docker Desktop.exe 由用户手动启动,WSL docker-desktop distro Running
#   - 但 named pipe 未生成
#
# 推荐修复路径(用户侧):
#   1) 退出 Docker Desktop 全部进程(Get-Process | Where Name -match docker | Stop-Process -Force)
#   2) 等 10s
#   3) 重启 Docker Desktop(开始菜单快捷方式)
#   4) 等待右下角托盘图标变绿色(~30-60s,首次启动 1-2min)
#   5) pwsh scripts/diag-docker-bridge.ps1 → 应输出 OK
# ============================================================================

$ErrorActionPreference = 'Continue'
$dockerExe = "C:\Program Files\Docker\Docker\resources\bin\docker.exe"

function Write-Section($title) {
    Write-Host ""
    Write-Host "=== $title ===" -ForegroundColor Cyan
}

function Test-PipeAvailable($pipeName) {
    # PowerShell 5.1 没有原生 Test-Path 对 named pipe 的支持,
    # 用 [System.IO.Directory]::Exists 试探(对真实 pipe 抛 IOException,
    # 那是 pipe 存在的迹象)。但 harness 沙箱可能拦了网络路径访问,
    # 因此这里用 .NET FileStream open 探测(超时 1s)。
    try {
        $job = Start-Job -ScriptBlock {
            param($p)
            try {
                $fs = [System.IO.File]::Open($p, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
                $fs.Close()
                return "EXISTS"
            } catch [System.IO.FileNotFoundException] {
                return "NOT_FOUND"
            } catch [System.IO.IOException] {
                return "BUSY"  # 进程占用 = pipe 存在
            } catch {
                return "ERROR: $($_.Exception.Message)"
            }
        } -ArgumentList $pipeName
        if (Wait-Job $job -Timeout 3) {
            $r = Receive-Job $job
            Remove-Job $job -Force
            return $r
        }
        Remove-Job $job -Force
        return "TIMEOUT"
    } catch {
        return "ERROR: $($_.Exception.Message)"
    }
}

Write-Section "1) Docker CLI 版本与 Context"
& $dockerExe version 2>&1 | Select-Object -First 6
& $dockerExe context ls 2>&1

Write-Section "2) Docker Desktop / Backend 进程"
$procs = Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -match "docker|Docker" }
if ($procs) {
    $procs | Select-Object ProcessName, Id | Format-Table -AutoSize
} else {
    Write-Host "  [WARN] 没有 docker 相关进程在运行" -ForegroundColor Yellow
}

Write-Section "3) WSL Distro 状态"
$wslOut = wsl -l -v 2>&1
$wslOut | Out-String | Write-Host
$dockerDesktopRunning = $false
if ($wslOut -match "docker-desktop\s+Running") {
    $dockerDesktopRunning = $true
    Write-Host "  [OK] docker-desktop WSL distro is Running" -ForegroundColor Green
} else {
    Write-Host "  [FAIL] docker-desktop WSL distro NOT Running" -ForegroundColor Red
}

Write-Section "4) com.docker.service 状态"
$svc = Get-Service com.docker.service -ErrorAction SilentlyContinue
if ($svc) {
    $svc | Select-Object Name, Status, StartType | Format-Table -AutoSize
    if ($svc.Status -eq "Running") {
        Write-Host "  [OK] com.docker.service Running" -ForegroundColor Green
    } else {
        Write-Host "  [WARN] com.docker.service NOT Running" -ForegroundColor Yellow
    }
} else {
    Write-Host "  [WARN] com.docker.service 不存在(可能被卸载 / 未安装 Docker Desktop)" -ForegroundColor Yellow
}

Write-Section "5) Named pipe 探测"
$p1 = Test-PipeAvailable "\\.\pipe\dockerDesktopLinuxEngine"
Write-Host "  dockerDesktopLinuxEngine: $p1"
$p2 = Test-PipeAvailable "\\.\pipe\docker_engine"
Write-Host "  docker_engine: $p2"

Write-Section "6) docker info (8s timeout)"
$job = Start-Job -ScriptBlock { & $using:dockerExe info 2>&1 | Select-Object -First 15 }
$infoResult = if (Wait-Job $job -Timeout 8) { Receive-Job $job } else { Stop-Job $job; "TIMEOUT 8s" }
Remove-Job $job -Force -ErrorAction SilentlyContinue
$infoResult | Out-String | Write-Host
$serverUp = $infoResult -match "Server:\s*Version"

Write-Section "7) 结论"
if ($serverUp) {
    Write-Host "  [OK] Docker daemon 已就绪" -ForegroundColor Green
    exit 0
} elseif ($dockerDesktopRunning -and $p1 -ne "NOT_FOUND") {
    Write-Host "  [OK-ish] docker-desktop Running 且 pipe 有响应,但 docker info 慢" -ForegroundColor Yellow
    Write-Host "         通常继续等 30-60s 后会好" -ForegroundColor Yellow
    exit 0
} else {
    Write-Host "  [BLOCKED] Docker daemon bridge 未就绪" -ForegroundColor Red
    Write-Host ""
    Write-Host "  修复指引(用户侧手动):" -ForegroundColor Yellow
    Write-Host "    1) 退出 Docker Desktop 全部进程:"
    Write-Host "         Get-Process | Where-Object {`$_.ProcessName -match 'docker|Docker'} | Stop-Process -Force"
    Write-Host "    2) 等 10s"
    Write-Host "    3) 开始菜单启动 'Docker Desktop'"
    Write-Host "    4) 等右下角托盘图标变绿(首次启动 1-2min)"
    Write-Host "    5) 重跑: pwsh scripts/diag-docker-bridge.ps1"
    Write-Host ""
    Write-Host "  备选(临时绕过): 直接在 WSL Ubuntu 里跑 docker,绕过 Windows 桥:"
    Write-Host "    wsl -d Ubuntu -- docker ps"
    Write-Host ""
    exit 1
}
