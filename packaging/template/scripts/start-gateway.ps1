#Requires -Version 7.0
<#
.SYNOPSIS
    im1.0 启动网关（先自检配置，再拉起服务）。

.DESCRIPTION
    为什么不用「直接跑 bin/im-gateway.exe」：

    `AppConfig::load_from_paths` 里是 `let _ = dotenvy::dotenv();`
    （crates/im-common/src/config.rs:289）—— **返回值被丢弃**。也就是说
    `.env` 写坏了、编码不对、路径不对，dotenvy 都**静默失败**，服务照常启动
    然后报 `config load failed: internal error`。operator 完全看不出是
    「.env 没被读到」还是「.env 里某个值不对」。

    本脚本先跑 preflight 把这两种情况分开，再启动服务。

.PARAMETER SkipPreflight
    跳过自检（不建议）。

.PARAMETER Port
    覆盖 IM_HTTP_PORT。

.EXAMPLE
    pwsh -File scripts/start-gateway.ps1
    pwsh -File scripts/start-gateway.ps1 -Port 9090
#>
[CmdletBinding()]
param(
    [switch] $SkipPreflight,
    [int]    $Port = 0
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

$root = Split-Path -Parent $PSScriptRoot
$exe  = Join-Path $root 'bin\im-gateway.exe'
$env  = Join-Path $root '.env'

if (-not (Test-Path -LiteralPath $exe)) { throw "找不到 $exe —— 安装包不完整，请重新解压。" }
if (-not (Test-Path -LiteralPath $env)) { throw "找不到 $env —— 请先运行 scripts\install.ps1 生成配置。" }

if ($Port -gt 0) { $env:IM_HTTP_PORT = [string]$Port }

if (-not $SkipPreflight) {
    Write-Host ''
    Write-Host '>>> 启动前自检' -ForegroundColor Cyan
    & (Join-Path $PSScriptRoot 'preflight.ps1') -EnvFile $env
    if ($LASTEXITCODE -ne 0) {
        Write-Host ''
        Write-Host '自检未通过，已中止启动。' -ForegroundColor Red
        Write-Host '（服务本来也会起不来，但那时它只会说一句 internal error。）' -ForegroundColor DarkGray
        exit 1
    }
}

Write-Host ''
Write-Host ">>> 启动 im-gateway（Ctrl+C 停止）" -ForegroundColor Green
Write-Host "    工作目录: $root"
Write-Host "    .env    : $env"
$host.UI.RawUI.WindowTitle = 'im-gateway'
& $exe
exit $LASTEXITCODE
