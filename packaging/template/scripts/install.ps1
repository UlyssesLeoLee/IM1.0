#Requires -Version 7.0
<#
.SYNOPSIS
    im1.0 一键安装：生成 .env、跑迁移、自检。

.DESCRIPTION
    顺序是有讲究的：
      1. 生成 .env（从 env.example 复制，不覆盖已有）
      2. 跑自检 —— 必须在迁移**之前**，否则用一个错的连接串去建表
      3. 跑迁移（im-migrate）—— 迁移 SQL 由 `sqlx::migrate!` 编译期内嵌，
         所以包里的 im-migrate 自带的 schema 与该二进制**必然同版本**，
         不存在「包里的 SQL 比代码旧」这种最难排查的状态。

    迁移与服务的凭据权限是分开的：迁移要 DDL，服务只要 DML。

.PARAMETER SkipMigrate
    跳过数据库迁移（只做配置自检）。

.PARAMETER Force
    已存在 .env 时也覆盖（**会丢失现有密钥**，除非你确认）。

.EXAMPLE
    pwsh -File scripts/install.ps1
    pwsh -File scripts/install.ps1 -SkipMigrate
#>
[CmdletBinding()]
param(
    [switch] $SkipMigrate,
    [switch] $Force
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

$root      = Split-Path -Parent $PSScriptRoot
$example   = Join-Path $root 'config\env.example'
$envPath   = Join-Path $root '.env'
$migrateEx = Join-Path $root 'bin\im-migrate.exe'

# ---- 1. 生成 .env ----
Write-Host ''
Write-Host '=== [1/3] 生成配置 ===' -ForegroundColor Cyan
if ((Test-Path -LiteralPath $envPath) -and -not $Force) {
    Write-Host "  .env 已存在，保持不变（要重建请加 -Force，那会丢弃现有密钥）" -ForegroundColor Yellow
}
else {
    if (-not (Test-Path -LiteralPath $example)) { throw "找不到 $example —— 安装包不完整。" }
    Copy-Item -LiteralPath $example -Destination $envPath
    Write-Host "  已从 config\env.example 生成 .env" -ForegroundColor Green
    Write-Host ''
    Write-Host '  请把下面这些占位符换掉（下一步的自检会逐项点名，不会漏）:' -ForegroundColor Yellow
    Write-Host '    - IM_POSTGRES_URL     里的 CHANGE_ME'
    Write-Host '    - IM_JWT_SIGNING_KEYS 里的 CHANGE_ME（至少 32 字节随机串）'
    Write-Host '    - IM_REFRESH_PEPPER   里的 CHANGE_ME'
    Write-Host ''
    Write-Host '  生成随机密钥：'
    Write-Host '    -join ((1..48) | ForEach-Object { ''0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ''[(Get-Random -Maximum 62)] })'
    Write-Host ''
    # **刻意没有 Read-Host 确认**。
    # 商业产品最常见的安装方式是**非交互**的(CI job / Ansible / cloud-init /
    # 容器 entrypoint), 提示会挂在那儿或行为不确定; 而下一步的 preflight
    # 本来就会逐项点名还差什么, 提示不增加任何信息, 只增加一个卡死的点。
    Write-Host "  （如需现在编辑: notepad $envPath）" -ForegroundColor DarkGray
}

# ---- 2. 自检（必须在迁移之前）----
Write-Host ''
Write-Host '=== [2/3] 配置自检 ===' -ForegroundColor Cyan
& (Join-Path $PSScriptRoot 'preflight.ps1') -EnvFile $envPath -SkipBinaryCheck
if ($LASTEXITCODE -ne 0) {
    Write-Host ''
    Write-Host '  自检未通过，已中止。先修好配置再迁移 —— ' -ForegroundColor Red
    Write-Host '  用一个错的连接串去建表，失败信息会指向 schema 而不是配置。' -ForegroundColor DarkGray
    exit 1
}

# ---- 3. 迁移 ----
Write-Host ''
if ($SkipMigrate) {
    Write-Host '=== [3/3] 迁移（已按 -SkipMigrate 跳过）===' -ForegroundColor Cyan
    exit 0
}
Write-Host '=== [3/3] 数据库迁移 ===' -ForegroundColor Cyan
if (-not (Test-Path -LiteralPath $migrateEx)) { throw "找不到 $migrateEx —— 安装包不完整。" }

# im-migrate 自己读 IM_POSTGRES_URL（crates/im-migrate/src/lib.rs:146），
# 不用在命令行上传连接串 —— 避免密码出现在进程列表里。
$envLines = [System.IO.File]::ReadAllLines($envPath, [System.Text.UTF8Encoding]::new($false))
foreach ($line in $envLines) {
    $t = $line.Trim()
    if ($t -eq '' -or $t.StartsWith('#')) { continue }
    if ($t -match '^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$') {
        $v = $Matches[2].Trim()
        if ($v.Length -ge 2 -and (($v[0] -eq '"' -and $v[-1] -eq '"') -or ($v[0] -eq "'" -and $v[-1] -eq "'"))) {
            $v = $v.Substring(1, $v.Length - 2)
        }
        Set-Item -Path "Env:$($Matches[1])" -Value $v
    }
}

& $migrateEx
if ($LASTEXITCODE -ne 0) {
    Write-Host ''
    Write-Host "  迁移失败（exit=$LASTEXITCODE）。" -ForegroundColor Red
    Write-Host '  常见原因：连接串错 / 数据库不存在 / 账号没有 DDL 权限。' -ForegroundColor DarkGray
    exit $LASTEXITCODE
}

Write-Host ''
Write-Host '=== 安装完成 ===' -ForegroundColor Green
Write-Host '  启动:  pwsh -File scripts\start-gateway.ps1'
Write-Host '  文档:  docs\QUICKSTART.md'
Write-Host '  运维:  bin\jobctl.exe dlq list | replay | discard'
exit 0
