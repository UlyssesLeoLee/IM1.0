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

# ============================================================================
# 平台映射 —— 纯函数, 不读任何全局状态
# ============================================================================
#
# 为什么单独抽出来: 「包名后缀 + 二进制扩展名」是打包里唯一一处**平台相关**的
# 决策, 此前它是两行写死的 `-win-x64` 与 "$b.exe"。写死的后果不是「不优雅」,
# 是 **CI 上根本产不出包**: 集成方在 Linux/macOS 上拿不到任何二进制分发件。
#
# 纯函数 = 参数进、值出, 不碰 $IsWindows / $env:X / 当前目录。理由是可测:
# 只有纯函数才能在**任意**机器上被喂任意 (OSPlatform, Architecture) 组合 ——
# 而 CI runner 恰恰就是要在一台不是自己的机器上问「如果我是 Linux 我会得到什么」。
# 直接读全局状态的话, 这台 runner 永远只能验出它自己那一种组合。
#
# 契约:
#   Suffix     包名后缀, 沿用 Rust target triple 的约定 (win-x64/linux-x64/darwin-arm64)
#   Extension  可执行文件扩展名: Windows '.exe', 其余 '' (Unix 可执行文件无后缀)
#   RustTarget  cargo 完整 target triple, 供 BUILD-INFO 记录 (人可读的溯源信息)
#   IsWindows  决定文档/清理/示例该怎么写
#
# fail-closed: 不认识的 OS / 架构**抛异常**, 绝不编一个看起来合理的包名 ——
# 一个错后缀的包会让人以为「这个平台没做」, 而实际是「平台识别错了」。
function Get-PackageTarget {
    [CmdletBinding()]
    [OutputType([pscustomobject])]
    param(
        [Parameter(Mandatory = $true)] [string] $OSPlatform,
        [Parameter(Mandatory = $true)] [string] $Architecture
    )

    # switch 默认大小写不敏感, 故 'MacOS' / 'macos' 也认。
    $os = switch ($OSPlatform) {
        'Windows' { [pscustomobject]@{ Slug = 'win';    Ext = '.exe'; Triple = '{0}-pc-windows-msvc' } }
        'Linux'   { [pscustomobject]@{ Slug = 'linux';  Ext = '';     Triple = '{0}-unknown-linux-gnu' } }
        'macOS'   { [pscustomobject]@{ Slug = 'darwin'; Ext = '';     Triple = '{0}-apple-darwin' } }
        default {
            throw "不支持的操作系统: '$OSPlatform'。已实现 Windows / Linux / macOS; 其它平台没有经过验证的包名约定, 拒绝编造。"
        }
    }

    $arch = switch ($Architecture) {
        'X64'   { 'x64' }
        'Arm64' { 'arm64' }
        default {
            throw "不支持的架构: '$Architecture'。已实现 X64 / Arm64; 其它架构没有经过验证的包名约定, 拒绝编造。"
        }
    }

    $rustArch = if ($arch -eq 'x64') { 'x86_64' } else { 'aarch64' }

    [pscustomobject]@{
        OSPlatform  = $OSPlatform
        Architecture = $Architecture
        Suffix      = "$($os.Slug)-$arch"
        Extension   = $os.Ext
        RustTarget  = $os.Triple -f $rustArch
        IsWindows   = ($OSPlatform -eq 'Windows')
    }
}

# ---- 被 dot-source 时只取函数, 不执行下面的打包流程 ----
# 单测要 import 上面的函数, 而本脚本余下部分是**有副作用的**(Set-Location /
# cargo build / 删目录 / 写 zip)。不设这道闸, 测一个纯函数就会顺手把
# `dist/` 删一遍重建 —— 测试不该动生产路径。
# $MyInvocation.InvocationName 在 `pwsh -File x.ps1` / `& x.ps1` / `. x.ps1`
# 三种调用下分别是 <路径> / '&' / '.'(已实测)。
if ($MyInvocation.InvocationName -eq '.') { return }

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
