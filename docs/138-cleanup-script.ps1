# IM1.0 重复内容清理脚本 (per 138-dev-plan.md + Ulysses 2026-09-12 06:13 JST 拍板)
#
# 删除目标 (8.5 GB 重复内容):
#   1. D:\IM1.0-target-merge         (3.57 GB, cargo target 副本)
#   2. D:\IM1.0-target-probe         (0.85 GB, cargo target 副本)
#   3. D:\IM1.0-wt                   (4.09 GB, cargo target 副本, 含 target-testkit/)
#   4. D:\IMIM0                      (0.005 MB, 旧版源码 8/23, IM1.0 main 已有新版本)
#   5. D:\IM1.0\target\cc.log         (cargo check 临时 log, 2.3 KB)
#   6. D:\IM1.0\target\ct.log         (cargo test 临时 log, 14.4 KB)
#   7. D:\IM1.0\target\ct2.log        (cargo test 临时 log, 19.9 KB)
#   8. D:\IM1.0\target\cc_out.log     (空)
#   9. D:\IM1.0\target\cc_err.log     (空)
#  10. D:\IM1.0\target\cargo_check.out (空)
#  11. D:\IM1.0\target\cargo_check.err (空)
#
# 工具: mavis-trash (可恢复删除, Recycle Bin 机制)
#       注: agent 进程被 system hard-block, 此脚本由 Ulysses 手动执行
#
# 用法: 右键 -> Run with PowerShell, 或: pwsh -NoProfile -File 138-cleanup-script.ps1

$ErrorActionPreference = 'Stop'

# 路径列表 (按 Ulysses 拍板 + 138-dev-plan.md §4 + §6)
$paths = @(
    'D:\IM1.0-target-merge',
    'D:\IM1.0-target-probe',
    'D:\IM1.0-wt',
    'D:\IMIM0',
    'D:\IM1.0\target\cc.log',
    'D:\IM1.0\target\ct.log',
    'D:\IM1.0\target\ct2.log',
    'D:\IM1.0\target\cc_out.log',
    'D:\IM1.0\target\cc_err.log',
    'D:\IM1.0\target\cargo_check.out',
    'D:\IM1.0\target\cargo_check.err'
)

# 优先用绝对路径 (避免 -NoProfile 不加载 PATH)
$trashPath = Join-Path $env:USERPROFILE '.minimax\bin\mavis-trash.cmd'
if (-not (Test-Path $trashPath)) {
    # fallback: 用 Get-Command (依赖 PATH)
    $trashCmd = Get-Command mavis-trash -ErrorAction SilentlyContinue
    if ($trashCmd) {
        $trashPath = $trashCmd.Source
    } else {
        Write-Host ('[ERROR] mavis-trash not found at {0}' -f $trashPath) -ForegroundColor Red
        Write-Host '也未在 PATH 中找到 mavis-trash, 请检查安装' -ForegroundColor Yellow
        exit 1
    }
}
Write-Host ('[INFO] trash tool: {0}' -f $trashPath) -ForegroundColor DarkGray

Write-Host '========================================' -ForegroundColor Cyan
Write-Host 'IM1.0 重复内容清理 (mavis-trash 可恢复)' -ForegroundColor Cyan
Write-Host '========================================' -ForegroundColor Cyan
Write-Host ''

$totalSize = 0
foreach ($p in $paths) {
    if (Test-Path $p) {
        $size = (Get-ChildItem $p -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
        $sizeMB = [Math]::Round($size / 1MB, 2)
        $totalSize += $size
        Write-Host ('  [{0,8} MB] {1}' -f $sizeMB, $p) -ForegroundColor White
    } else {
        Write-Host ('  [   SKIP   ] {0} (不存在)' -f $p) -ForegroundColor DarkGray
    }
}

$totalMB = [Math]::Round($totalSize / 1MB, 2)
$totalGB = [Math]::Round($totalSize / 1GB, 2)
Write-Host ''
Write-Host ('  总计: {0} MB ({1} GB)' -f $totalMB, $totalGB) -ForegroundColor Yellow
Write-Host ''

$confirm = Read-Host '确认删除以上内容到 Recycle Bin? (yes/no)'
if ($confirm -ne 'yes') {
    Write-Host '[CANCELLED]' -ForegroundColor Yellow
    exit 0
}

Write-Host ''
Write-Host '[START]' -ForegroundColor Green
foreach ($p in $paths) {
    if (Test-Path $p) {
        Write-Host ('  trashing: {0}' -f $p) -ForegroundColor Yellow
        & $trashPath $p 2>&1 | ForEach-Object { Write-Host ('    ' + $_) }
    } else {
        Write-Host ('  skip: {0} (不存在)' -f $p) -ForegroundColor DarkGray
    }
}

Write-Host ''
Write-Host '[DONE] 全部 mavis-trash 完成, 可在 Recycle Bin 恢复' -ForegroundColor Green
Write-Host '[VERIFY] 重新跑 ''Get-ChildItem D:\IM1.0*'' 确认已清空' -ForegroundColor Cyan
