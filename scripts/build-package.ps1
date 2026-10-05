#Requires -Version 7.0
<#
.SYNOPSIS
    组装 im1.0 分发包（安装包）并生成 SHA256SUMS。

.DESCRIPTION
    为什么要**脚本化**而不是手工 zip：

    手工打的包无法回答「这个包是从哪个 commit 出的、里面装的是哪几个
    二进制」。所以本脚本把版本号、commit、构建时间、三个二进制的 SHA256
    一并写进包内的 BUILD-INFO.txt，operator 报障时贴这一个文件就够。

    包里装什么（以及**不**装什么）：

      bin/        im-gateway / im-migrate / jobctl（release 产物）
      config/     env.example（配置契约模板）
      scripts/    install / preflight / start-gateway
      docs/       QUICKSTART / openapi.json / asyncapi.json
      LICENSE     Apache-2.0

      **不含** migrations/ —— `sqlx::migrate!` 已把 SQL 编译进 im-migrate，
      带上目录反而会让人以为可以从目录跑迁移，出现「目录里的 SQL 比
      二进制旧」这种最难排查的状态。

      **不含** .env / 任何密钥。

.PARAMETER Version
    包版本号。默认取 workspace 的 version。

.PARAMETER OutDir
    输出目录。默认 `dist/`。

.PARAMETER TargetDir
    cargo 产物目录。默认取 $env:CARGO_TARGET_DIR 或 `target`。

.PARAMETER SkipBuild
    不重新编译，直接用已有的 release 产物。

.EXAMPLE
    pwsh -File scripts/build-package.ps1
    pwsh -File scripts/build-package.ps1 -Version 0.1.0 -SkipBuild
#>
[CmdletBinding()]
param(
    [string] $Version = '',
    [string] $OutDir = 'dist',
    [string] $TargetDir = '',
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

# ---- 版本号 ----
if (-not $Version) {
    $cargoToml = [System.IO.File]::ReadAllText((Join-Path $repo 'Cargo.toml'), [System.Text.UTF8Encoding]::new($false))
    if ($cargoToml -notmatch '(?m)^\s*version\s*=\s*"([^"]+)"') { throw '无法从 Cargo.toml 解析 version' }
    $Version = $Matches[1]
}
$sha = (git rev-parse --short HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'git rev-parse 失败' }
$dirty = (git status --porcelain | Measure-Object).Count
$stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')

# ---- 目标目录 ----
if (-not $TargetDir) {
    $TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }
}
$relBin = Join-Path $TargetDir 'release'

$pkgName = "im1.0-$Version-win-x64"
$pkgRoot = Join-Path $repo (Join-Path $OutDir $pkgName)

Write-Host ''
Write-Host "=== 组装 $pkgName ===" -ForegroundColor Cyan
Write-Host "  版本   : $Version"
Write-Host "  commit : $sha$(if ($dirty -gt 0) { "  (工作树有 $dirty 处未提交改动 —— 这不该发生, 请先提交或 stash)" })"
Write-Host "  产物   : $relBin"

# ---- 构建 ----
if (-not $SkipBuild) {
    Write-Host ''
    Write-Host '--- cargo build --release ---' -ForegroundColor Cyan
    & cargo build --release --locked -p im-gateway -p im-migrate -p jobctl
    if ($LASTEXITCODE -ne 0) { throw "cargo build 失败 (exit=$LASTEXITCODE)" }
}

# ---- 清空并建目录树 ----
if (Test-Path -LiteralPath $pkgRoot) {
    # 同 test-preflight: `dist/` 是 .gitignore 覆盖的构建产物目录, 整个重建,
    # 走回收站没有意义。build-package 是 **Windows 专用**脚本, 但清理不该
    # 依赖本机的可恢复删除工具 —— 那会让脚本在别的机器/CI 上跑不了。
    Write-Host ''
    Write-Host "  已存在 $pkgName，正在删除并重建" -ForegroundColor Yellow
    Remove-Item -LiteralPath $pkgRoot -Recurse -Force -ErrorAction Stop
}
foreach ($d in @('bin', 'config', 'scripts', 'docs')) {
    New-Item -ItemType Directory -Path (Join-Path $pkgRoot $d) -Force | Out-Null
}

# ---- 拷二进制 ----
$bins = @{}
foreach ($b in @('im-gateway', 'im-migrate', 'jobctl')) {
    $src = Join-Path $relBin "$b.exe"
    if (-not (Test-Path -LiteralPath $src)) { throw "缺少产物 $src —— 先构建再打包" }
    Copy-Item -LiteralPath $src -Destination (Join-Path $pkgRoot "bin\$b.exe")
    $bins[$b] = (Get-FileHash -LiteralPath $src -Algorithm SHA256).Hash
    Write-Host "  bin\$b.exe"
}

# ---- 拷模板 / 文档 ----
Copy-Item -LiteralPath 'packaging\template\env.example'  -Destination (Join-Path $pkgRoot 'config\env.example')
Copy-Item -LiteralPath 'packaging\template\INSTALL.md'    -Destination (Join-Path $pkgRoot 'INSTALL.md')
foreach ($s in @('install.ps1', 'preflight.ps1', 'start-gateway.ps1')) {
    Copy-Item -LiteralPath "packaging\template\scripts\$s" -Destination (Join-Path $pkgRoot "scripts\$s")
}
Copy-Item -LiteralPath 'docs\api\QUICKSTART.md' -Destination (Join-Path $pkgRoot 'docs\QUICKSTART.md')
Copy-Item -LiteralPath 'docs\api\openapi.json'  -Destination (Join-Path $pkgRoot 'docs\openapi.json')
Copy-Item -LiteralPath 'docs\api\asyncapi.json' -Destination (Join-Path $pkgRoot 'docs\asyncapi.json')
$license = Get-ChildItem -Path . -Filter 'LICENSE*' -File | Select-Object -First 1
if ($license) { Copy-Item -LiteralPath $license.FullName -Destination (Join-Path $pkgRoot 'LICENSE') }
Write-Host '  config\env.example / scripts\*.ps1 / docs\* / INSTALL.md'

# ---- BUILD-INFO ----
# rustc 是**可选**的: 本仓没有 rust-toolchain.toml, 而打包常常发生在
# 「只装了 cargo 运行时 / 复用别处产物 / CI 缓存」的环境里。拿不到就写
# 未知, 不能因为一条元信息让整个打包失败 —— 那会让「打包」在某些环境下
# 永远做不成, 而它本来是可以做的。
$rustcVer = '未知（构建环境未提供 rustc）'
if (Get-Command rustc -ErrorAction SilentlyContinue) {
    $rustcVer = (& rustc --version)
    if ($LASTEXITCODE -ne 0 -or -not $rustcVer) { $rustcVer = '未知（rustc 调用失败）' }
}

$bi = @()
$bi += "im1.0 分发包构建信息"
$bi += ''
$bi += "版本      : $Version"
$bi += "commit     : $sha"
$bi += "构建时间(UTC): $stamp"
$bi += "构建平台  : windows x86_64"
$bi += "Rust 工具链: $rustcVer"
$bi += ''
$bi += '二进制 SHA256:'
foreach ($k in $bins.Keys) { $bi += ("  {0,-12} {1}" -f $k, $bins[$k]) }
$bi += ''
$bi += '本包不含 migrations/ 目录: 迁移 SQL 由 sqlx::migrate! 编译期内嵌进'
$bi += 'im-migrate 二进制，二进制与其携带的 schema 必然同版本。'
$bi += ''
$bi += '校验整个包:'
$bi += '  Get-FileHash <解压出的文件> -Algorithm SHA256'
$bi += '  与本文件及 SHA256SUMS.txt 比对'
[System.IO.File]::WriteAllLines((Join-Path $pkgRoot 'BUILD-INFO.txt'), $bi, [System.Text.UTF8Encoding]::new($false))

# ---- SHA256SUMS（覆盖包内每个文件）----
$sumLines = Get-ChildItem -LiteralPath $pkgRoot -Recurse -File |
    Sort-Object FullName |
    ForEach-Object {
        $rel = $_.FullName.Substring($pkgRoot.Length + 1).Replace('\', '/')
        '{0}  {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower(), $rel
    }
[System.IO.File]::WriteAllLines((Join-Path $pkgRoot 'SHA256SUMS.txt'), $sumLines, [System.Text.UTF8Encoding]::new($false))

# ---- 压成 zip ----
$zip = Join-Path $repo (Join-Path $OutDir "$pkgName.zip")
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force -ErrorAction Stop }
Compress-Archive -Path $pkgRoot -DestinationPath $zip -CompressionLevel Optimal
$zipHash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
$zipInfo = Get-Item -LiteralPath $zip
Write-Host ''
Write-Host '=== 完成 ===' -ForegroundColor Green
Write-Host "  目录 : $pkgRoot"
Write-Host "  压缩 : $zip  ($([math]::Round($zipInfo.Length / 1MB, 2)) MB)"
Write-Host "  SHA256: $zipHash"
Write-Host ''
Write-Host '  发布前请人工确认:'
Write-Host '   1. 包里**没有** .env 或任何密钥（SHA256SUMS.txt 里应只有 env.example）'
Write-Host '   2. BUILD-INFO.txt 里的 commit 与你要发布的 commit 一致'
Write-Host '   3. 在一台**没有** Rust 工具链的机器上跑 scripts\install.ps1 -SkipMigrate 能过自检'
exit 0
